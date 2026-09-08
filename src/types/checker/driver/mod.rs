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
    throwables_to_register,
};
use super::builtin_types::{
    inject_builtin_date_period, inject_builtin_datetime, inject_builtin_reflection,
    program_may_reference_date_period, program_may_reference_datetime,
    program_may_reference_reflection,
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
use super::builtin_stdclass::inject_builtin_stdclass;
use super::builtin_user_filter::inject_builtin_user_filter;
use super::schema::{
    build_class_info_recursive, build_enum_info, build_interface_info_recursive,
    drop_unresolvable_attribute_arg_refs, validate_deferred_class_constants,
    validate_deferred_declaration_defaults,
};
use super::yield_validation::validate_yield_contexts;
use super::{CheckOptions, Checker};

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
) -> Result<(Checker, TypeEnv), CompileError> {
    let mut checker = Checker::new(target);
    checker.strict_locals = options.strict_locals;
    // Program-wide and computed once, BEFORE any body is walked: the top-level `unset` that has to
    // consult it can sit textually above the `function w() { global $a; }` that makes the name
    // program-global. Shared with EIR lowering's `all_global_var_names` so the two sides cannot
    // drift — see `crate::global_decls`, whose preamble also records the measured reason the veto
    // must NOT see further than lowering does.
    checker.program_global_names = crate::global_decls::collect_global_var_names(program);
    let mut errors = Vec::new();

    errors.extend(validate_yield_contexts(program));

    checker.collect_function_decls(program, &mut errors);

    let (mut flattened_classes, mut flattened_enums, flatten_errors) = flatten_classes(program);
    errors.extend(flatten_errors);
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
                crate::span::Span::dummy(),
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
            name,
            extends,
            properties,
            methods,
            constants,
        } = &stmt.kind
        {
            let interface_key = php_symbol_key(name);
            if interface_map
                .keys()
                .any(|existing| php_symbol_key(existing) == interface_key)
                || class_map
                    .keys()
                    .any(|existing| php_symbol_key(existing) == interface_key)
            {
                errors.push(CompileError::new(
                    stmt.span,
                    &format!("Duplicate interface declaration: {}", name),
                ));
                continue;
            }
            // An interface has no single parent class, so `self`/`static` resolve to the interface
            // itself; `parent` is left untouched (it is meaningless in an interface contract).
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
    let register_datetime = program_may_reference_datetime(program);
    let register_date_period = program_may_reference_date_period(program);
    let mut wanted_throwables = throwables_to_register(program, register_spl, register_reflection);
    if register_datetime {
        // Synthetic DateTime/DatePeriod overload adapters raise this class from checker-owned
        // method bodies, so the source-only throwable scan cannot discover the dependency.
        wanted_throwables.insert("ArgumentCountError".to_string());
    }
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
    if register_datetime {
        inject_builtin_datetime(&mut interface_map, &mut class_map, uses_tz_introspection);
    }
    if let Err(error) = inject_builtin_interfaces(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    // DatePeriod implements Iterator (registered just above) and references DateTime/DateInterval,
    // so it can only be registered when they are.
    if register_date_period {
        inject_builtin_date_period(&mut class_map, uses_tz_introspection);
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
    if let Err(error) = inject_builtin_spl_classes(
        &mut interface_map,
        &mut class_map,
        register_spl,
        register_date_period,
    ) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_stdclass(&mut class_map) {
        errors.extend(error.flatten());
    }
    // PHP's only way to write a stream filter is a class extending this one, which spells the
    // name; `stream_filter_register` is consulted as well.
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
        if let Err(error) = build_interface_info_recursive(
            &interface_name,
            &interface_map,
            &class_map,
            &mut checker,
            &mut next_interface_id,
            &mut building_interfaces,
        ) {
            errors.extend(error.flatten());
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
        if let Err(error) = build_class_info_recursive(
            &class_name,
            &class_map,
            &mut checker,
            &mut next_class_id,
            &mut building,
        ) {
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
                constants,
                enum_used_traits,
                enum_trait_aliases,
                stmt.span,
                &mut checker,
                &mut next_class_id,
            ) {
                errors.extend(error.flatten());
            }
        }
    }
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

    checker.prescan_extern_decls(program, &mut errors);

    let (_, initial_top_level_errors) = checker.check_top_level_program(program);

    checker.resolve_unchecked_functions(&mut errors);
    // Enum method bodies are not part of `flattened_classes` (enums are registered separately via
    // the enum schema pass), so they would otherwise skip body checking entirely. Flatten them
    // into method-checkable units here — their signatures already live in `checker.classes`.
    let mut methods_to_check = flattened_classes.clone();
    methods_to_check.extend(flatten_enum_methods(program, &flattened_enums));
    checker.type_check_methods_until_stable(&methods_to_check, &mut errors)?;
    patch_builtin_spl_storage_signatures(&mut checker);
    apply_implicit_stringable_interfaces(&mut checker.classes);

    let (final_global_env, final_top_level_errors) = checker.check_top_level_program(program);
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

    if !errors.is_empty() {
        return Err(CompileError::from_many(errors));
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
