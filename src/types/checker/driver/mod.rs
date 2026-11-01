//! Purpose:
//! Orchestrates the type-checker pipeline after parsing and name resolution.
//! Sequences initialization, declaration collection, top-level checking, externs, and function bodies.
//!
//! Called from:
//! - `crate::types::check()`
//!
//! Key details:
//! - Ordering is semantic: schemas and builtin metadata must exist before bodies and call sites are validated.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, StmtKind};
use crate::types::{traits::flatten_classes, TypeEnv};

use super::builtin_class_gate::{
    program_may_reference_fiber, program_may_reference_generator, program_may_reference_user_filter,
    program_may_register_user_filter, throwables_to_register,
};
use super::builtin_types::{
    inject_builtin_date_period, inject_builtin_datetime, inject_builtin_reflection,
    program_may_reference_datetime, program_may_reference_reflection,
    inject_builtin_throwables,
    patch_builtin_exception_signatures,
    patch_builtin_fiber_signatures, patch_builtin_reflection_signatures,
    patch_magic_method_signatures, InterfaceDeclInfo,
};
use super::builtin_enums::inject_builtin_enums;
use super::builtin_interfaces::{apply_implicit_stringable_interfaces, inject_builtin_interfaces};
use super::builtin_iterators::{inject_builtin_iterators, patch_builtin_generator_signatures};
use super::builtin_json::{inject_builtin_json_interfaces, patch_builtin_json_signatures};
use super::builtin_spl_classes::{
    inject_builtin_spl_classes, patch_builtin_spl_storage_signatures,
};
use super::builtin_spl_exceptions::inject_builtin_spl_exceptions;
use super::builtin_stdclass::inject_builtin_core_object_classes;
use super::builtin_user_filter::inject_builtin_user_filter;
use super::schema::{
    build_class_info_recursive, build_enum_info, build_interface_info_recursive,
    drop_unresolvable_attribute_arg_refs, validate_deferred_class_constants,
    validate_deferred_declaration_defaults, validate_enum_trait_requirements,
    expand_enum_interfaces, validate_enum_interface_contracts,
    validate_trait_property_defaults,
};
use super::yield_validation::validate_yield_contexts;
use super::{CheckOptions, Checker};

mod constants;
mod declaration_metadata;
mod externs;
mod functions;
mod init;
mod top_level;

use declaration_metadata::{
    collect_declared_trait_constants, collect_declared_trait_methods,
    collect_declared_trait_names, flatten_enum_methods,
    substitute_relative_class_types_in_constants,
    substitute_relative_class_types_in_flattened,
    substitute_relative_class_types_in_flattened_enums,
    substitute_relative_class_types_in_methods,
};

/// Orchestrates the full type-checker pipeline after parsing and name resolution.
///
/// Initializes the `Checker` and `TypeEnv`, then runs in order:
/// 1. Yield-context validation
/// 2. Function-declaration collection
/// 3. Class/interface map construction (including builtins via injection)
/// 4. Recursive class/interface info building
/// 5. Enum declaration processing
/// 6. Builtin signature patching
/// 7. Extern declaration prescanning
/// 8. Top-level program checking (twice: initial pass for errors that stabilize, then final)
/// 9. Method body type-checking to stability
/// 10. Implicit Stringable interface application
///
/// Returns `Ok((Checker, TypeEnv))` on success or `Err(CompileError)` if any phase reports errors.
/// The `Checker` carries resolved class/interface/enum/function metadata; `TypeEnv` holds the global type environment.
/// `options` is copied onto the constructed `Checker` (e.g. `strict_locals`) before any phase runs.
pub(super) fn check_types_impl(
    program: &Program,
    target: Target,
    options: CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<(Checker, TypeEnv), CompileError> {
    // The 19 s of "Checking types" has never been split, and it is ten documented phases with a
    // fixed point in the middle and a top-level pass run twice. `ELEPHC_TYPECHECK_TIMES=1`
    // prints where the time is before anything is changed on a guess.
    let trace = std::env::var("ELEPHC_TYPECHECK_TIMES").is_ok();
    let start = std::time::Instant::now();
    let mut mark = std::time::Instant::now();
    let lap = |label: &str, mark: &mut std::time::Instant| {
        if trace {
            eprintln!(
                "[elephc-typecheck] {label}={:.2}s",
                mark.elapsed().as_secs_f64()
            );
        }
        *mark = std::time::Instant::now();
    };
    let mut checker = Checker::new(target);
    checker.lint_warnings = options.lint;
    checker.strict_locals = options.strict_locals;
    // Installed BEFORE any body is walked, because `new Box(5)` is resolved while walking one.
    // Empty on every path that does not run `generics::monomorphize`, which is what makes those
    // paths behave exactly as they did before this feature.
    checker.class_templates = generics.class_templates.clone();
    checker.method_templates = generics.method_templates.clone();
    // Program-wide and computed once, BEFORE any body is walked: the top-level `unset` that has to
    // consult it can sit textually above the `function w() { global $a; }` that makes the name
    // program-global. Shared with EIR lowering's `all_global_var_names` so the two sides cannot
    // drift — see `crate::global_decls`, whose preamble also records the measured reason the veto
    // must NOT see further than lowering does.
    checker.program_global_names = crate::global_decls::collect_global_var_names(program);
    // Program-wide and computed once, for the same reason as the line above and with an even
    // sharper need: both routes that let a class appear at run time can sit textually BELOW the
    // `new` that needs them. `spl_autoload_register(...)` in a bootstrap and the `new` inside a
    // function called afterwards is the ordinary Symfony shape, and a point-in-time flag walking
    // top to bottom would still be `false` at the `new`.
    //
    // The two routes, and why each is here. `includes_runtime_php` is an `include`/`require`
    // whose path is not a literal: the file is chosen at run time and its declarations are
    // executed by the interpreter. `spl_autoload_register` is the loader itself. `introspects` is
    // NOT consulted wholesale — it also covers `get_defined_functions`, which supplies no class,
    // and deferring on it would give up a real compile-time diagnostic for nothing. A literal
    // `eval()` needs nothing here either: `eval_barrier_active` already covers the code AFTER it,
    // and code BEFORE it is refused by PHP too.
    // Published for the class-family gates below, which ask the same question of the same slice.
    let published_usage = crate::prelude_prune::usage::publish(program);
    let program_usage = published_usage.usage();
    checker.program_defers_unknown_classes = program_usage.includes_runtime_php
        || options.registers_autoloader
        // The register call that the autoload collector REJECTED is still in the program, and it
        // is still an autoloader as far as the runtime is concerned: it reaches the interpreter's
        // SPL callback chain instead of the compile-time rule list. Both forms have to count.
        || program_usage.references("spl_autoload_register");
    // A transparent literal fragment reaches builtins only and cannot call back into the
    // program, so it does not widen anything; every other eval still does.
    checker.program_has_interpreted_callers =
        checker.program_defers_unknown_classes || program_usage.opaque_eval;
    let mut errors = Vec::new();

    errors.extend(validate_yield_contexts(program));
    errors.extend(validate_trait_property_defaults(program));

    constants::prescan(&mut checker, program);
    checker.collect_function_decls(program, &mut errors);

    let (mut flattened_classes, mut flattened_enums, flatten_errors) = flatten_classes(program);
    errors.extend(flatten_errors);
    (checker.trait_imported_properties, checker.trait_imported_constructors) =
        super::schema::collect_trait_default_origins(program, &flattened_classes);
    // A prelude parsed from PHP text gives its classes real spans, so the statement's source
    // mode is what tells them apart from the program's own classes (`ClassInfo::is_internal`).
    checker.internal_class_decls = program
        .iter()
        .filter(|stmt| stmt.source_mode == crate::source::SourceMode::Internal)
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    // Resolve the relative class types `self`/`static`/`parent` in every member type annotation
    // now that inheritance and trait flattening have settled the concrete enclosing class. This
    // single pass feeds the schema signatures, the body-check pass, and codegen (which all read
    // the flattened method/property declarations), so no later stage sees a symbolic `self`.
    substitute_relative_class_types_in_flattened(&mut flattened_classes);
    substitute_relative_class_types_in_flattened_enums(&mut flattened_enums);
    let declared_traits = collect_declared_trait_names(program);
    let declared_trait_methods = collect_declared_trait_methods(program);
    let declared_trait_constants = collect_declared_trait_constants(program);
    let mut seen_classes = HashSet::new();
    let mut class_map = HashMap::new();
    for class in &flattened_classes {
        let key = php_symbol_key(&class.name);
        if !seen_classes.insert(key) {
            errors.push(CompileError::new(
                class.span,
                &format!("Duplicate class declaration: {}", class.name),
            ));
            continue;
        }
        class_map.insert(class.name.clone(), class.clone());
    }
    let mut interface_map: HashMap<String, InterfaceDeclInfo> = HashMap::new();
    checker.declared_classes = class_map.keys().cloned().collect();
    for stmt in program {
        if let StmtKind::InterfaceDecl {
            generics: _,
            name,
            extends,
            properties,
            methods,
            constants,
        } = &stmt.kind
        {
            if interface_map
                .keys()
                .any(|existing| existing.eq_ignore_ascii_case(name))
                || class_map
                    .keys()
                    .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                errors.push(CompileError::new(
                    stmt.span,
                    &format!("Duplicate interface declaration: {}", name),
                ));
                continue;
            }
            // An interface has no single parent class, so `self`/`static` resolve to the interface
            // itself; `parent` is left untouched (it is meaningless in an interface contract).
            if let Some(doc_comment) = crate::parser::classlike_doc_comment(stmt.span, name) {
                checker.interface_doc_comments.insert(name.clone(), doc_comment);
            }
            let mut interface_methods = methods.clone();
            let mut interface_constants = constants.clone();
            substitute_relative_class_types_in_methods(&mut interface_methods, name, None);
            substitute_relative_class_types_in_constants(&mut interface_constants, name, None);
            interface_map.insert(
                name.clone(),
                InterfaceDeclInfo {
                    name: name.clone(),
                    extends: extends
                        .iter()
                        .map(|name| name.as_str().to_string())
                        .collect(),
                    properties: properties.clone(),
                    methods: interface_methods,
                    span: stmt.span,
                    constants: interface_constants,
                },
            );
        }
    }
    // Both surface gates are pure functions of the program, and the throwable gate needs their
    // answers: SPL container helpers throw five exceptions by id and the Reflection helpers throw
    // ReflectionException, none of them naming a class any scan of the source can see. They are
    // computed here and reused at their own injection sites below.
    let register_spl = crate::types::checker::builtin_spl_classes::program_may_reference_spl(program);
    let register_reflection = program_may_reference_reflection(program);
    let mut wanted_throwables = throwables_to_register(program, register_spl, register_reflection);
    // Fiber and FiberError ride in the same set because `inject_builtin_throwables` owns their
    // declarations. Nothing raises a FiberError without a Fiber, so one answer covers both.
    if program_may_reference_fiber(program) {
        wanted_throwables.insert("Fiber".to_string());
        wanted_throwables.insert("FiberError".to_string());
    }
    if let Err(error) =
        inject_builtin_throwables(&mut interface_map, &mut class_map, &wanted_throwables)
    {
        errors.extend(error.flatten());
        return Err(CompileError::from_many(errors));
    }
    // The tz_prelude (injected upstream only when the program uses timezone
    // introspection) declares `timezone_location_get`. Its presence gates the
    // three `DateTimeZone` introspection methods, which reference the elephc_tz
    // bridge and must not be added — and linked — for every DateTimeZone program.
    let uses_tz_introspection = checker.has_function_decl_folded("timezone_location_get");
    // Pay-for-use, on the same reasoning and with the same loud failure mode as the SPL and
    // Reflection gates below: fifteen classes and an interface that the checker flattens,
    // patches and validates for a program that never writes a date type, each class also
    // claiming a slot in every dense `_class_*` metadata table — 76% of the type-check phase of
    // a trivial program. `program_may_reference_datetime` carries the measurement.
    // Gated at the call site rather than inside, because this injection has no redeclaration
    // check to keep running — it inserts only names the program has not already declared.
    let register_datetime = program_may_reference_datetime(program);
    if register_datetime {
        inject_builtin_datetime(&mut interface_map, &mut class_map, uses_tz_introspection);
    }
    if let Err(error) = inject_builtin_interfaces(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    // DatePeriod implements Iterator (registered just above) and references DateTime/DateInterval,
    // so it can only be registered when they are.
    if register_datetime {
        inject_builtin_date_period(&mut class_map);
    }
    // Pay-for-use like the families around it, but per-class rather than all-or-nothing: naming
    // one of these registers it and its ancestors, and nothing else. Eight of the thirteen have
    // no producer anywhere in elephc, so a program that never writes the name cannot reach them.
    if let Err(error) =
        inject_builtin_spl_exceptions(&mut interface_map, &mut class_map, &wanted_throwables)
    {
        errors.extend(error.flatten());
    }
    // A `yield` materializes a Generator no source line names; that, and spelling the type, are
    // the only two routes to it. See `program_may_reference_generator`.
    if let Err(error) = inject_builtin_iterators(
        &mut interface_map,
        &mut class_map,
        program_may_reference_generator(program),
    ) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_json_interfaces(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    // Pay-for-use, because the checker's walk over these 41 classes is 27 ms — 54% of the
    // type-check phase of a trivial program. See `program_may_reference_spl` for the measurement
    // and for why under-detecting here is a readable compile error rather than a miscompile.
    // The redeclaration check inside runs regardless of the decision. (`register_spl` is computed
    // above, because the throwable gate needs it too.)
    if let Err(error) = inject_builtin_spl_classes(&mut interface_map, &mut class_map, register_spl)
    {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_core_object_classes(&mut class_map) {
        errors.extend(error.flatten());
    }
    // PHP's only way to write a stream filter is a class extending this one, which spells the
    // name; `stream_filter_register` is consulted as well.
    checker.user_filters_reachable = program_may_register_user_filter(program);
    checker.user_filter_registered =
        crate::types::checker::builtin_class_gate::user_filter_registered_classes(program);
    if let Err(error) =
        inject_builtin_user_filter(&mut class_map, program_may_reference_user_filter(program))
    {
        errors.extend(error.flatten());
    }
    // Pay-for-use, on the same reasoning and with the same loud failure mode as the SPL gate
    // above: a program that never names a Reflection type should not pay for the checker to
    // flatten, patch and validate fourteen of them. `program_may_reference_reflection` carries
    // the measurement. The redeclaration check inside runs regardless of the decision.
    // (`register_reflection` is computed above, because the throwable gate needs it too.)
    if let Err(error) = inject_builtin_reflection(
        &interface_map,
        &mut class_map,
        &declared_traits,
        register_reflection,
    ) {
        errors.extend(error.flatten());
    }
    checker.declared_classes = class_map.keys().cloned().collect();
    checker.declared_class_parents = class_map
        .iter()
        .map(|(name, class)| (name.clone(), class.extends.clone()))
        .collect();
    checker.declared_class_interfaces = class_map
        .iter()
        .map(|(name, class)| (name.clone(), class.implements.clone()))
        .collect();
    checker.declared_interfaces = interface_map.keys().cloned().collect();
    checker.declared_traits = declared_traits.clone();
    checker.declared_trait_methods = declared_trait_methods;
    checker.declared_trait_constants = declared_trait_constants;
    // Enum names must resolve as types in member positions (property and
    // promoted-constructor-param types), which are checked during the class
    // schema pass — before the enum-processing phase populates `enums`. Pre-
    // declare them alongside classes (mirrors the later insert in `schema::enums`).
    for stmt in program {
        if let StmtKind::EnumDecl { name, .. } = &stmt.kind {
            checker.declared_classes.insert(name.clone());
        }
    }
    // Packed and extern classes are types that class members name too: `public buffer<Vec2> $data`,
    // `public ptr<Point> $p`, a method parameter, or a generic argument (`Store<Vec2>` substitutes
    // `buffer<T>` into a member). Their layouts must therefore exist before the interface and
    // class schema passes below resolve member signatures. Duplicate detection against ordinary
    // classes reads `declared_classes`, which is already complete here.
    checker.prescan_extern_decls(program, &mut errors);

    let mut next_interface_id = 0u64;
    let mut building_interfaces = HashSet::new();
    // Sorted: `interface_map` is a HashMap, whose iteration order is randomized per
    // process. Interface ids are handed out in this order and are baked into the
    // generated assembly, so an unsorted walk makes two compilations of the SAME
    // source produce different output — which defeats any content-addressed cache.
    // Nothing below depends on the order: `build_interface_info_recursive` pulls its own
    // parents, so the walk decides numbering and nothing else.
    let mut interface_names: Vec<String> = interface_map.keys().cloned().collect();
    interface_names.sort();
    for interface_name in interface_names {
        let result = build_interface_info_recursive(
            &interface_name,
            &interface_map,
            &class_map,
            &mut checker,
            &mut next_interface_id,
            &mut building_interfaces,
        );
        building_interfaces.clear();
        if let Err(error) = result {
            errors.extend(error.within_declaration(interface_name.as_str()).flatten());
        }
    }

    let mut next_class_id = 0u64;
    let mut building = HashSet::new();
    // Sorted for the same reason as `interface_names` above: class ids are assigned in
    // this walk order and end up as immediates and `.quad` values in the emitted
    // assembly, so a HashMap-ordered walk is a reproducibility hole. They also reach the
    // `_class_*` metadata tables and the object header each `new` stamps.
    let mut class_names: Vec<String> = class_map.keys().cloned().collect();
    class_names.sort();
    for class_name in class_names {
        let result = build_class_info_recursive(
            &class_name,
            &class_map,
            &mut checker,
            &mut next_class_id,
            &mut building,
        );
        building.clear();
        if let Err(error) = result {
            errors.extend(error.flatten());
        }
    }
    if let Err(error) = inject_builtin_enums(program, &mut checker, &mut next_class_id) {
        errors.extend(error.flatten());
    }
    for stmt in program {
        if let StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
            implements,
            methods,
            constants,
            ..
        } = &stmt.kind
        {
            let enum_methods = flattened_enums
                .get(name)
                .map(|flattened| flattened.methods.as_slice())
                .unwrap_or(methods.as_slice());
            let enum_constants = flattened_enums
                .get(name)
                .map(|flattened| flattened.constants.as_slice())
                .unwrap_or(constants.as_slice());
            let enum_used_traits = flattened_enums
                .get(name)
                .map(|flattened| flattened.used_traits.as_slice())
                .unwrap_or(&[]);
            let enum_trait_aliases = flattened_enums
                .get(name)
                .map(|flattened| flattened.trait_aliases.as_slice())
                .unwrap_or(&[]);
            if let Err(error) = build_enum_info(
                name,
                backing_type.as_ref(),
                cases,
                implements,
                enum_methods,
                methods,
                enum_constants,
                constants,
                enum_used_traits,
                enum_trait_aliases,
                &stmt.attributes,
                stmt.span,
                stmt.source_mode == crate::source::SourceMode::Internal,
                &mut checker,
                &mut next_class_id,
            ) {
                errors.extend(error.flatten());
            }
        }
    }
    // Publish all transitive interfaces before validating hints that mention another enum.
    let mut enum_units: Vec<_> = flattened_enums.values()
        .filter(|unit| checker.enums.contains_key(&unit.name)).collect();
    enum_units.sort_by(|left, right| left.name.cmp(&right.name));
    let mut expanded = HashSet::new();
    for enum_unit in &enum_units {
        match expand_enum_interfaces(&mut checker, enum_unit) {
            Ok(()) => { expanded.insert(enum_unit.name.clone()); }
            Err(error) => errors.extend(error.flatten()),
        }
    }
    for enum_unit in enum_units {
        if expanded.contains(&enum_unit.name) {
            if let Err(error) = validate_enum_interface_contracts(&mut checker, enum_unit) {
                errors.extend(error.flatten());
            }
            if let Err(error) = validate_enum_trait_requirements(&checker, program, enum_unit) {
                errors.extend(error.flatten());
            }
        }
    }
    checker.box_untyped_interface_method_params();
    report_class_id_inventory(&checker);
    errors.extend(validate_deferred_declaration_defaults(
        &mut checker,
        &flattened_classes,
        program,
    ));
    errors.extend(validate_deferred_class_constants(
        &mut checker,
        &flattened_classes,
        &interface_map,
        &flattened_enums,
        program,
    ));
    // All class/interface/enum metadata now exists, so deferred symbolic
    // attribute-argument references can be checked for resolvability. Drop any
    // the EIR backend cannot lower (e.g. built-in `Attribute::TARGET_CLASS`) so
    // the attribute still compiles, just without reflectable arguments.
    drop_unresolvable_attribute_arg_refs(&mut checker);

    patch_builtin_exception_signatures(&mut checker);
    patch_builtin_fiber_signatures(&mut checker);
    patch_builtin_json_signatures(&mut checker);
    patch_builtin_reflection_signatures(&mut checker);
    patch_builtin_generator_signatures(&mut checker);
    patch_builtin_spl_storage_signatures(&mut checker);
    patch_magic_method_signatures(&mut checker);

    // Every class, interface and enum is registered, and no body has been checked: a violated
    // bound is reported now, alone, instead of as whatever the instantiated body trips over.
    if let Err(error) =
        checker.verify_class_type_argument_bounds_before_bodies(&generics.class_type_argument_bounds)
    {
        errors.extend(error.flatten());
        return Err(CompileError::from_many(errors));
    }

    // Property layout is an input to inferred returns, not a late EIR-only adjustment. Do this
    // before the first free-function body is resolved so its callers see the same array ABI.
    for class_info in checker.classes.values_mut() {
        class_info.normalize_untyped_array_property_storage();
    }

    // Runs before ANY body is walked, and has to: a declared by-reference parameter the callee
    // widens needs a boxed cell on BOTH sides of every call, and the walk order below does not
    // guarantee the callee is reached first — the top level is checked before
    // `resolve_unchecked_functions` ever touches a free function's body. See
    // `Checker::scan_widened_ref_params`.
    lap("declarations", &mut mark);
    checker.scan_widened_ref_params(program, &flattened_classes);
    lap("widened_ref_scan", &mut mark);

    let (_, initial_top_level_errors) = checker.check_top_level_program(program);
    lap("top_level_first", &mut mark);

    checker.resolve_unchecked_functions(&mut errors);
    lap("unchecked_functions", &mut mark);
    if checker.eval_native_callables_reachable {
        if let Err(error) = checker.promote_eval_native_variadic_containers() {
            errors.extend(error.flatten());
        }
    }
    // After every function has a signature and every direct call has been seen, so the pass can
    // tell a function the program only ever reaches dynamically from one a call site taught real
    // parameter types to (issue #576).
    checker.widen_dynamic_only_passthrough_returns();
    // Enum method bodies are not part of `flattened_classes` (enums are registered separately via
    // the enum schema pass), so they would otherwise skip body checking entirely. Flatten them
    // into method-checkable units here — their signatures already live in `checker.classes`.
    let mut methods_to_check = flattened_classes.clone();
    methods_to_check.extend(flatten_enum_methods(program, &flattened_enums));
    lap("collect_method_units", &mut mark);
    checker.type_check_methods_until_stable(&methods_to_check, &mut errors)?;
    // A free function is resolved at its first call, BEFORE any method body is checked, so a
    // function returning a method's result (`function f(Fluent $f) { return $f->name; }` over
    // `__get`) kept the method's placeholder `int` as its own return type and handed its caller
    // a boxed pointer as an integer. Re-resolve those returns now that method types are settled,
    // and recheck the methods when one moved.
    // A map walk may visit every caller before its callee, so each round advances only one
    // forwarding edge. Allow the full declaration chain instead of leaving longer chains on
    // their scalar seeds after three rounds. Stable programs still exit as soon as nothing moves.
    let return_round_limit = checker.fn_decls.len().saturating_add(1).max(3);
    for _ in 0..return_round_limit {
        if !checker.reinfer_undeclared_function_returns() {
            break;
        }
        // Re-resolving narrows a return back to what its call-site parameter types imply, which
        // undid the dynamic-only widening above: `function opt($b = null) { return $b; }` called
        // as `opt()` and through `call_user_func('opt', "probe")` returned `Str`, and `opt()`
        // printed `""` instead of `NULL`. Re-apply it before the methods see the new returns.
        checker.widen_dynamic_only_passthrough_returns();
        checker.type_check_methods_until_stable(&methods_to_check, &mut errors)?;
    }
    checker.recheck_foreach_warning_functions(&mut errors);
    lap("methods_until_stable", &mut mark);
    checker.publish_unspecialized_method_param_seeds();
    patch_builtin_spl_storage_signatures(&mut checker);
    apply_implicit_stringable_interfaces(&mut checker.classes);
    // The implicit `Stringable` implementations just added change which classes satisfy an interface
    // receiver, so no dispatch answer memoized before this point may be reused.
    checker.subtype_dispatch_cache.borrow_mut().1.clear();
    lap("post_passes", &mut mark);

    let (final_global_env, final_top_level_errors) = checker.check_top_level_program(program);
    lap("top_level_final", &mut mark);
    for (initial_errors, final_errors) in initial_top_level_errors
        .into_iter()
        .zip(final_top_level_errors.into_iter())
    {
        if !final_errors.is_empty() {
            errors.extend(final_errors);
            continue;
        }
        if !Checker::can_suppress_initial_top_level_errors(&initial_errors) {
            errors.extend(initial_errors);
        }
    }

    checker.deduplicate_variant_signature_errors(&mut errors);
    if !errors.is_empty() {
        return Err(CompileError::from_many(errors));
    }

    // Every diagnostic that reads a declared by-reference parameter type has now run, so the
    // signature can stop describing the declaration and start describing the CELL — which for a
    // parameter the callee widens is boxed on both sides of the call. See
    // `Checker::publish_widened_ref_param_signatures`.
    checker.publish_widened_ref_param_signatures();
    if trace {
        eprintln!(
            "[elephc-typecheck] total={:.2}s",
            start.elapsed().as_secs_f64()
        );
    }

    Ok((checker, final_global_env))
}

/// Prints every class the checker registered, with its id, when `ELEPHC_CLASS_INVENTORY=1`.
///
/// The dense `_class_*` metadata tables are `max_class_id + 1` entries wide, and every id no
/// emitted class claims costs an 8-byte `-2` sentinel in each of roughly twenty-five tables.
/// Counting those slots in the emitted assembly says HOW MANY are wasted; only this says WHICH
/// classes hold them, which is what decides whether a registration is worth gating.
fn report_class_id_inventory(checker: &Checker) {
    if std::env::var("ELEPHC_CLASS_INVENTORY").as_deref() != Ok("1") {
        return;
    }
    let mut rows: Vec<(u64, &str)> = checker
        .classes
        .iter()
        .map(|(name, class_info)| (class_info.class_id, name.as_str()))
        .collect();
    rows.sort();
    eprintln!("class inventory: {} registered", rows.len());
    for (class_id, name) in rows {
        eprintln!("  {class_id:>3}  {name}");
    }
}

#[cfg(test)]
mod tests {
    fn foreach_warnings(source: &str) -> Vec<String> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::new(
                crate::codegen_support::platform::Platform::MacOS,
                crate::codegen_support::platform::Arch::AArch64,
            ),
        )
        .expect("check")
        .warnings
        .into_iter()
        .filter(|warning| warning.message.starts_with("foreach() argument"))
        .map(|warning| warning.message)
        .collect()
    }

    /// Declared void functions also need their loop diagnostics refreshed after methods settle.
    #[test]
    fn foreach_warning_uses_settled_forward_method_return() {
        let warnings = foreach_warnings(r#"<?php
class EntrySource {
    private $entries = ['one', 'two'];
    public function values() { return $this->entries; }
}
function walk(EntrySource $source): void {
    $entries = $source->values();
    foreach ($entries as $value) { echo $value; }
}
walk(new EntrySource());
"#);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// Callable-array narrowing must not retain a diagnostic emitted against the method seed.
    #[test]
    fn foreach_warning_uses_settled_indirect_method_return() {
        let warnings = foreach_warnings(r#"<?php
class Registry {
    private $entries = ['a' => ['one', 'two']];
    public function getEntries() { return $this->entries; }
    public function load($name) {}
}
function visit(array $callbacks): void {
    foreach ($callbacks as $callback) {
        if (is_array($callback) && $callback[0] instanceof Registry) {
            foreach ($callback[0]->getEntries() as $prefix => $items) {
                echo $prefix, ':', implode(',', $items);
            }
        }
    }
}
visit([[new Registry(), 'load']]);
"#);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// A nested closure has its own storage scope but must refresh with its enclosing function.
    #[test]
    fn foreach_warning_refreshes_nested_closure_subjects() {
        let warnings = foreach_warnings(r#"<?php
class EntrySource {
    private $entries = ['one', 'two'];
    public function values() { return $this->entries; }
}
function walk(EntrySource $source): void {
    $visit = function () use ($source): void {
        foreach ($source->values() as $value) { echo $value; }
    };
    $visit();
}
walk(new EntrySource());
"#);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// The diagnostic refresh must follow free-function return reinference as well as methods.
    #[test]
    fn foreach_warning_uses_settled_free_function_wrapper() {
        let warnings = foreach_warnings(r#"<?php
class EntrySource {
    private $entries = ['one', 'two'];
    public function values() { return $this->entries; }
}
function entries(EntrySource $source) { return $source->values(); }
function walk(EntrySource $source): void {
    foreach (entries($source) as $value) { echo $value; }
}
walk(new EntrySource());
"#);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// A long forwarding chain must settle even when the map walk visits callers first.
    #[test]
    fn foreach_warning_uses_settled_deep_function_chain() {
        let mut source = String::from(r#"<?php
class EntrySource {
    private $entries = ['one', 'two'];
    public function values() { return $this->entries; }
}
function entries0(EntrySource $source) { return $source->values(); }
"#);
        for index in 1..32 {
            source.push_str(&format!(
                "function entries{index}(EntrySource $source) {{ return entries{}($source); }}\n",
                index - 1,
            ));
        }
        source.push_str(r#"
function walk(EntrySource $source): void {
    foreach (entries31($source) as $value) { echo $value; }
}
walk(new EntrySource());
"#);
        let warnings = foreach_warnings(&source);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// Rewalking for diagnostics must not narrow a return widened for dynamic callers.
    #[test]
    fn foreach_warning_preserves_dynamic_return_widening() {
        let tokens = crate::lexer::tokenize(r#"<?php
class EntrySource {
    private $entries = ['one', 'two'];
    public function values() { return $this->entries; }
}
function opt($value = null) {
    $source = new EntrySource();
    foreach ($source->values() as $entry) { echo $entry; }
    return $value;
}
opt();
$callback = 'opt';
call_user_func($callback, 'probe');
"#).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let result = crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::new(
                crate::codegen_support::platform::Platform::MacOS,
                crate::codegen_support::platform::Arch::AArch64,
            ),
        ).expect("typecheck");
        assert_eq!(result.functions["opt"].return_type, crate::types::PhpType::Mixed);
        assert!(!result.warnings.iter().any(|warning| warning.message.starts_with("foreach() argument")));
    }

    /// Method rounds and both top-level walks replace their own provisional loop diagnostics.
    #[test]
    fn foreach_warning_uses_settled_method_and_top_level_subjects() {
        let warnings = foreach_warnings(r#"<?php
class Consumer {
    public function walk(EntrySource $source): void {
        foreach ($source->values() as $value) { echo $value; }
    }
}
class EntrySource {
    private $entries = ['one', 'two'];
    public function values() { return $this->entries; }
}
$source = new EntrySource();
foreach ($source->values() as $value) { echo $value; }
(new Consumer())->walk($source);
"#);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// Refreshing speculative diagnostics must keep real scalar loops, including uncalled bodies.
    #[test]
    fn foreach_warning_keeps_real_scalar_sites() {
        let warnings = foreach_warnings(r#"<?php
class ScalarSource { public function value() { return 42; } }
function walk(ScalarSource $source): void {
    foreach ($source->value() as $value) { echo $value; }
}
function uncalled(): void { foreach (false as $value) { echo $value; } }
walk(new ScalarSource());
foreach ('text' as $value) { echo $value; }
"#);
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        for kind in ["int", "false", "string"] {
            assert!(warnings.iter().any(|warning| warning.contains(&format!("{kind} given"))), "{warnings:?}");
        }
    }

    /// Every enum `inject_builtin_enums` registers for a target is a builtin pointer target for
    /// that target and no other name is, so the list the packed and extern prescan consults
    /// (`is_builtin_enum_name`) cannot drift from the injection, including the macOS-only
    /// `Pcntl\QosClass`.
    #[test]
    fn builtin_enum_pointer_targets_match_the_injected_enums() {
        use std::collections::HashSet;

        use crate::codegen_support::platform::{AppleVariant, Arch, Platform, Target};
        use crate::types::checker::builtin_enums::{inject_builtin_enums, is_builtin_enum_name};

        let targets = [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new(Platform::Linux, Arch::X86_64),
            Target::new(Platform::Linux, Arch::AArch64),
        ];
        for target in targets {
            let mut checker = super::Checker::new(target);
            let before: HashSet<String> = checker.enums.keys().cloned().collect();
            let mut next_class_id = 1;
            inject_builtin_enums(&Vec::new(), &mut checker, &mut next_class_id)
                .expect("builtin enums inject into an empty program");
            let injected: HashSet<String> = checker
                .enums
                .keys()
                .filter(|name| !before.contains(*name))
                .cloned()
                .collect();
            for name in ["SortDirection", "PropertyHookType", "Pcntl\\QosClass", "Unrelated"] {
                assert_eq!(
                    is_builtin_enum_name(&checker, name),
                    injected.contains(name),
                    "{name} on {target:?}",
                );
            }
        }
    }

    /// Imported abstract factory requirements never replace synthesized concrete metadata.
    #[test]
    fn enum_metadata_keeps_synthesized_factories_concrete() {
        use crate::codegen_support::platform::{Arch, Platform, Target};
        use crate::parser::ast::{ClassMethod, Visibility};
        use crate::span::Span;

        let mut checker = super::Checker::new(Target::new(Platform::MacOS, Arch::AArch64));
        let method = |name: &str, is_static: bool| ClassMethod {
            name: name.to_string(),
            visibility: Visibility::Public,
            is_static,
            is_abstract: true,
            is_final: false,
            has_body: false,
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: Vec::new(),
            span: Span::dummy(),
            attributes: Vec::new(),
        };
        let mut next_class_id = 1;
        crate::types::checker::schema::insert_enum_metadata(
            "State",
            Some(crate::types::PhpType::Int),
            Vec::new(),
            &[],
            &[method("from", true), method("tryFrom", true)],
            &[],
            &[],
            &[],
            Span::dummy(),
            false,
            &mut checker,
            &mut next_class_id,
        )
        .expect("record enum metadata");
        let class = checker.classes.get("State").expect("enum class metadata");
        assert!(class.abstract_methods.is_empty());
        assert!(class.abstract_static_methods.is_empty());
        for name in ["from", "tryfrom"] {
            assert_eq!(class.static_method_impl_classes.get(name).map(String::as_str), Some("State"));
            assert_eq!(class.static_method_declaring_classes.get(name).map(String::as_str), Some("State"));
            assert_eq!(class.static_methods[name].params[0].1, crate::types::PhpType::Int);
            assert!(!class.method_decls.iter().any(|decl| decl.name.eq_ignore_ascii_case(name)));
        }
    }

    /// Abstract enum method declarations retain their status in parallel class metadata.
    #[test]
    fn enum_metadata_records_abstract_instance_and_static_methods() {
        use crate::codegen_support::platform::{Arch, Platform, Target};
        use crate::parser::ast::{ClassMethod, Visibility};
        use crate::span::Span;

        let mut checker = super::Checker::new(Target::new(Platform::MacOS, Arch::AArch64));
        let method = |name: &str, is_static: bool| ClassMethod {
            name: name.to_string(),
            visibility: Visibility::Public,
            is_static,
            is_abstract: true,
            is_final: false,
            has_body: false,
            type_params: Vec::new(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: Vec::new(),
            span: Span::dummy(),
            attributes: Vec::new(),
        };
        let mut next_class_id = 1;
        crate::types::checker::schema::insert_enum_metadata(
            "State",
            None,
            Vec::new(),
            &[],
            &[method("instance", false), method("statik", true)],
            &[],
            &[],
            &[],
            Span::dummy(),
            false,
            &mut checker,
            &mut next_class_id,
        )
        .expect("record enum metadata");
        let class = checker.classes.get("State").expect("enum class metadata");
        assert!(class.abstract_methods.contains("instance"));
        assert!(class.abstract_static_methods.contains("statik"));
        assert!(!class.method_impl_classes.contains_key("instance"));
        assert!(!class.static_method_impl_classes.contains_key("statik"));
        assert_eq!(class.method_declaring_classes.get("instance").map(String::as_str), Some("State"));
        assert_eq!(class.static_method_declaring_classes.get("statik").map(String::as_str), Some("State"));
    }

    /// The outer method/function/top-level fixpoint must converge when a method's untyped
    /// variadic is both promoted to the descriptor container (a first-class callable inside a
    /// method body) and called with a string from the top level.
    ///
    /// The method pass promoted `is`'s collector to `array<mixed>`, the top-level re-check joined
    /// the `'api/*'` call site back into `array<string>` (`Str` absorbs `Mixed` in the coercion
    /// join), and the two alternated until the round budget ran out; on Laravel that budget is
    /// thousands of rounds. The collector must end as the wider `array<mixed>`, the only type the
    /// descriptor invoker and both call sites agree on.
    #[test]
    fn promoted_method_variadic_is_not_narrowed_back_by_a_top_level_call() {
        let source = r#"<?php
class Req {
    public function is(...$patterns) {
        foreach ($patterns as $p) { if ($p === "api/x") return true; }
        return false;
    }
}
class Mw {
    public function check(Req $r) {
        $f = $r->is(...);
        return $f("api/x");
    }
}
$g = fn (Req $r) => $r->is('api/*');
var_dump($g(new Req()));
var_dump((new Mw())->check(new Req()));
"#;
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let program = crate::name_resolver::resolve(program).expect("name resolve");
        let checked = crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::new(
                crate::codegen_support::platform::Platform::MacOS,
                crate::codegen_support::platform::Arch::AArch64,
            ),
        )
        .expect("check");
        let is_sig = checked
            .classes
            .get("Req")
            .and_then(|class| class.methods.get("is"))
            .expect("Req::is signature");
        assert_eq!(
            is_sig.params.last().map(|(_, ty)| ty.clone()),
            Some(crate::types::PhpType::Array(Box::new(crate::types::PhpType::Mixed))),
            "the promoted collector must stay array<mixed>"
        );
    }

    /// Class ids must not depend on HashMap iteration order.
    ///
    /// THIS TEST CANNOT BE WRITTEN AS "check the same source twice and compare". Rust seeds its
    /// hasher once per PROCESS, so two checks inside one test observe the same iteration order
    /// and agree whether or not the driver sorts — the bug this guards was only visible by
    /// running the compiler binary twice. So it asserts the property directly instead: classes
    /// with no inheritance between them take ids in sorted name order. Eight of them make an
    /// accidental pass a 1-in-40320 event.
    ///
    /// Declared in reverse so that "ids follow declaration order" fails it too.
    #[test]
    fn unrelated_classes_take_ids_in_a_stable_order() {
        let source = "<?php class Hotel {} class Golf {} class Foxtrot {} class Echo_ {} \
                      class Delta {} class Charlie {} class Bravo {} class Alpha {}";
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let checked = crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::new(
                crate::codegen_support::platform::Platform::MacOS,
                crate::codegen_support::platform::Arch::AArch64,
            ),
        )
        .expect("check");

        let mut declared: Vec<(u64, &str)> = [
            "Alpha", "Bravo", "Charlie", "Delta", "Echo_", "Foxtrot", "Golf", "Hotel",
        ]
        .iter()
        .map(|name| {
            let class_info = checked
                .classes
                .get(*name)
                .unwrap_or_else(|| panic!("{name} should be registered"));
            (class_info.class_id, *name)
        })
        .collect();
        declared.sort();

        let by_id: Vec<&str> = declared.iter().map(|(_, name)| *name).collect();
        assert_eq!(
            by_id,
            vec!["Alpha", "Bravo", "Charlie", "Delta", "Echo_", "Foxtrot", "Golf", "Hotel"],
            "class ids should follow sorted names, not hash or declaration order"
        );
    }
}
