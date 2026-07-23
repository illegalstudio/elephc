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

use crate::codegen::platform::Platform;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind};
use crate::types::{
    callable_wrapper_sig,
    traits::{flatten_classes, FlattenedClass},
    FunctionSig, PhpType, TypeEnv,
};

use super::builtin_types::{
    inject_builtin_date_period, inject_builtin_datetime, inject_builtin_reflection,
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
use super::Checker;

mod externs;
mod functions;
mod init;
mod top_level;

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
pub(super) fn check_types_impl(
    program: &Program,
    target_platform: Platform,
) -> Result<(Checker, TypeEnv), CompileError> {
    let mut checker = Checker::new(target_platform);
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
    if let Err(error) = inject_builtin_throwables(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    // The tz_prelude (injected upstream only when the program uses timezone
    // introspection) declares `timezone_location_get`. Its presence gates the
    // three `DateTimeZone` introspection methods, which reference the elephc_tz
    // bridge and must not be added — and linked — for every DateTimeZone program.
    let uses_tz_introspection = checker.has_function_decl_folded("timezone_location_get");
    inject_builtin_datetime(&mut interface_map, &mut class_map, uses_tz_introspection);
    if let Err(error) = inject_builtin_interfaces(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    // DatePeriod implements Iterator (registered just above) and references DateTime/DateInterval.
    inject_builtin_date_period(&mut class_map);
    if let Err(error) = inject_builtin_spl_exceptions(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_iterators(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_json_interfaces(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_spl_classes(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_stdclass(&mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_user_filter(&mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_reflection(&interface_map, &mut class_map, &declared_traits)
    {
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
    let interface_names: Vec<String> = interface_map.keys().cloned().collect();
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
    let class_names: Vec<String> = class_map.keys().cloned().collect();
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

    let (global_env, initial_top_level_errors) = checker.check_top_level_program(program);

    checker.resolve_unchecked_functions(&mut errors);
    // Enum method bodies are not part of `flattened_classes` (enums are registered separately via
    // the enum schema pass), so they would otherwise skip body checking entirely. Flatten them
    // into method-checkable units here — their signatures already live in `checker.classes`.
    let mut methods_to_check = flattened_classes.clone();
    methods_to_check.extend(flatten_enum_methods(program, &flattened_enums));
    // Method bodies seed their base environment from the top-level env. A top-level
    // local that error recovery poisoned as `Mixed` (its initializer failed to type)
    // must not leak into that seed: merged with a same-named method local it would
    // stay `Mixed` and spawn a spurious follow-on error (e.g. a bogus return-type
    // mismatch). Drop those poisoned names so recovery never adds noise to an already
    // failing program's method diagnostics.
    let method_seed_env = if checker.failed_assignment_targets.is_empty() {
        global_env.clone()
    } else {
        global_env
            .iter()
            .filter(|(name, _)| !checker.failed_assignment_targets.contains(*name))
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect()
    };
    checker.type_check_methods_until_stable(&methods_to_check, &method_seed_env, &mut errors)?;
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

/// Collects source-declared trait names recursively, including namespace blocks.
fn collect_declared_trait_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_declared_trait_names_into(program, &mut names);
    names
}

/// Pushes recursive source-declared trait names into `names`.
fn collect_declared_trait_names_into(program: &Program, names: &mut HashSet<String>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => {
                names.insert(name.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_declared_trait_names_into(body, names);
            }
            _ => {}
        }
    }
}

/// Collects source-declared trait method signatures recursively, including namespace blocks.
fn collect_declared_trait_methods(
    program: &Program,
) -> HashMap<String, HashMap<String, FunctionSig>> {
    let mut methods = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                methods: trait_methods,
                ..
            } => {
                methods.insert(
                    name.clone(),
                    trait_methods
                        .iter()
                        .map(|method| {
                            (
                                php_symbol_key(&method.name),
                                trait_method_reflection_sig(method),
                            )
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                methods.extend(collect_declared_trait_methods(body));
            }
            _ => {}
        }
    }
    methods
}

/// Collects source-declared trait constant names recursively, including namespace blocks.
fn collect_declared_trait_constants(program: &Program) -> HashMap<String, HashSet<String>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| constant.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constants(body));
            }
            _ => {}
        }
    }
    constants
}

/// Builds the reflection-visible signature for a direct trait method.
///
/// Trait direct reflection only needs parameter names, defaults, by-reference
/// flags, variadic shape, and declared-type presence; class-relative type names
/// are resolved when the trait is flattened into a concrete class.
fn trait_method_reflection_sig(method: &ClassMethod) -> FunctionSig {
    let params = method
        .params
        .iter()
        .map(|(name, type_ann, _, _)| {
            (
                name.clone(),
                if type_ann.is_some() {
                    PhpType::Mixed
                } else {
                    PhpType::Int
                },
            )
        })
        .collect();
    let defaults = method
        .params
        .iter()
        .map(|(_, _, default, _)| default.clone())
        .collect();
    let mut ref_params: Vec<bool> = method
        .params
        .iter()
        .map(|(_, _, _, by_ref)| *by_ref)
        .collect();
    if method.variadic.is_some() {
        ref_params.push(method.variadic_by_ref);
    }
    callable_wrapper_sig(&FunctionSig {
        params,
        param_type_exprs: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.clone())
            .chain(method.variadic.iter().map(|_| method.variadic_type.clone()))
            .collect(),
        param_attributes: method.param_attributes.clone(),
        defaults,
        return_type: PhpType::Mixed,
        declared_return: method.return_type.is_some(),
        by_ref_return: method.by_ref_return,
        ref_params,
        declared_params: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.is_some())
            .chain(
                method
                    .variadic
                    .iter()
                    .map(|_| method.variadic_type.is_some()),
            )
            .collect(),
        variadic: method.variadic.clone(),
        deprecation: None,
    })
}

/// Builds method-checkable `FlattenedClass` units for every `enum` in the program so their method
/// bodies go through the same validation as class methods. Enum signatures are already registered
/// in `checker.classes` by the enum schema pass; these units only carry the names and method
/// bodies the method-check pass needs. The relative types `self`/`static` resolve to the enum
/// itself (enums have no parent).
fn flatten_enum_methods(
    program: &[Stmt],
    flattened_enums: &HashMap<String, FlattenedClass>,
) -> Vec<FlattenedClass> {
    let mut units = Vec::new();
    for stmt in program {
        if let StmtKind::EnumDecl {
            name,
            implements,
            methods,
            constants,
            ..
        } = &stmt.kind
        {
            if let Some(flattened) = flattened_enums.get(name) {
                units.push(flattened.clone());
                continue;
            }
            let mut flattened = FlattenedClass {
                name: name.clone(),
                span: stmt.span,
                extends: None,
                implements: implements
                    .iter()
                    .map(|name| name.as_str().to_string())
                    .collect(),
                is_abstract: false,
                is_final: true,
                is_readonly_class: false,
                properties: Vec::new(),
                methods: methods.clone(),
                attributes: stmt.attributes.clone(),
                constants: constants.clone(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            };
            substitute_relative_class_types_in_methods(&mut flattened.methods, name, None);
            units.push(flattened);
        }
    }
    units
}

/// Resolves the relative class types `self`/`static`/`parent` to concrete class names across
/// every flattened class's method parameter, method return, and property type annotations.
///
/// `self`/`static` resolve to the flattened class itself and `parent` to its `extends` target.
/// Because trait methods are already merged into the using class at this point, a trait method's
/// `self` correctly resolves to the using class rather than the trait. Annotations with no
/// relative type are left untouched.
fn substitute_relative_class_types_in_flattened(classes: &mut [FlattenedClass]) {
    for class in classes.iter_mut() {
        let self_class = class.name.clone();
        let parent = class.extends.clone();
        let parent_ref = parent.as_deref();
        substitute_relative_class_types_in_methods(&mut class.methods, &self_class, parent_ref);
        for property in class.properties.iter_mut() {
            if let Some(ty) = property.type_expr.as_mut() {
                *ty = ty.substitute_relative_class_types(&self_class, parent_ref);
            }
        }
        substitute_relative_class_types_in_constants(
            &mut class.constants,
            &self_class,
            parent_ref,
        );
    }
}

/// Resolves relative class types inside flattened enum methods.
fn substitute_relative_class_types_in_flattened_enums(enums: &mut HashMap<String, FlattenedClass>) {
    for enum_unit in enums.values_mut() {
        let self_class = enum_unit.name.clone();
        substitute_relative_class_types_in_methods(&mut enum_unit.methods, &self_class, None);
        substitute_relative_class_types_in_constants(&mut enum_unit.constants, &self_class, None);
    }
}

/// Rewrites `self`/`static`/`parent` type annotations on class constants after
/// composition and inheritance have established the concrete owner.
fn substitute_relative_class_types_in_constants(
    constants: &mut [crate::parser::ast::ClassConst],
    self_class: &str,
    parent: Option<&str>,
) {
    for constant in constants {
        if let Some(type_expr) = constant.type_expr.as_mut() {
            *type_expr = type_expr.substitute_relative_class_types(self_class, parent);
        }
    }
}

/// Rewrites `self`/`static`/`parent` type annotations on a slice of methods by delegating to
/// `ClassMethod::substitute_relative_class_types`.
///
/// Used for user classes after trait/inheritance flattening, interfaces, and enums.
fn substitute_relative_class_types_in_methods(
    methods: &mut [ClassMethod],
    self_class: &str,
    parent: Option<&str>,
) {
    for method in methods.iter_mut() {
        method.substitute_relative_class_types(self_class, parent);
    }
}
