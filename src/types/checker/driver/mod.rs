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
    traits::{flatten_classes, FlattenedClass},
    TypeEnv,
};

use super::builtin_types::{
    inject_builtin_date_period, inject_builtin_datetime, inject_builtin_reflection,
    inject_builtin_throwables,
    patch_builtin_exception_signatures,
    patch_builtin_fiber_signatures, patch_builtin_reflection_signatures,
    patch_magic_method_signatures, InterfaceDeclInfo,
};
use super::builtin_enums::inject_builtin_enums;
use super::builtin_interfaces::{
    apply_implicit_stringable_interfaces, inject_builtin_interfaces,
};
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

    let (mut flattened_classes, flatten_errors) = flatten_classes(program);
    errors.extend(flatten_errors);
    // Resolve the relative class types `self`/`static`/`parent` in every member type annotation
    // now that inheritance and trait flattening have settled the concrete enclosing class. This
    // single pass feeds the schema signatures, the body-check pass, and codegen (which all read
    // the flattened method/property declarations), so no later stage sees a symbolic `self`.
    substitute_relative_class_types_in_flattened(&mut flattened_classes);
    let declared_traits: HashSet<String> = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
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
            substitute_relative_class_types_in_methods(&mut interface_methods, name, None);
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
                    constants: constants.clone(),
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
    if let Err(error) =
        inject_builtin_reflection(&interface_map, &mut class_map, &declared_traits)
    {
        errors.extend(error.flatten());
    }
    checker.declared_classes = class_map.keys().cloned().collect();
    checker.declared_interfaces = interface_map.keys().cloned().collect();

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
        } = &stmt.kind
        {
            if let Err(error) = build_enum_info(
                name,
                backing_type.as_ref(),
                cases,
                implements,
                methods,
                constants,
                stmt.span,
                &mut checker,
                &mut next_class_id,
            ) {
                errors.extend(error.flatten());
            }
        }
    }
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
    methods_to_check.extend(flatten_enum_methods(program));
    checker.type_check_methods_until_stable(&methods_to_check, &global_env, &mut errors)?;
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

/// Resolves the relative class types `self`/`static`/`parent` to concrete class names across
/// every flattened class's method parameter, method return, and property type annotations.
///
/// `self`/`static` resolve to the flattened class itself and `parent` to its `extends` target.
/// Because trait methods are already merged into the using class at this point, a trait method's
/// `self` correctly resolves to the using class rather than the trait. Annotations with no
/// relative type are left untouched.
/// Builds method-checkable `FlattenedClass` units for every `enum` in the program so their method
/// bodies go through the same validation as class methods. Enum signatures are already registered
/// in `checker.classes` by the enum schema pass; these units only carry the names and method
/// bodies the method-check pass needs. The relative types `self`/`static` resolve to the enum
/// itself (enums have no parent).
fn flatten_enum_methods(program: &[Stmt]) -> Vec<FlattenedClass> {
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
            let mut flattened = FlattenedClass {
                name: name.clone(),
                extends: None,
                implements: implements.iter().map(|name| name.as_str().to_string()).collect(),
                is_abstract: false,
                is_final: true,
                is_readonly_class: false,
                properties: Vec::new(),
                methods: methods.clone(),
                attributes: stmt.attributes.clone(),
                constants: constants.clone(),
                used_traits: Vec::new(),
            };
            substitute_relative_class_types_in_methods(&mut flattened.methods, name, None);
            units.push(flattened);
        }
    }
    units
}

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
    }
}

/// Rewrites the relative class types `self`/`static`/`parent` in each method's parameter and
/// return type annotations to `self_class`/`parent`. Shared by class and interface processing.
fn substitute_relative_class_types_in_methods(
    methods: &mut [ClassMethod],
    self_class: &str,
    parent: Option<&str>,
) {
    for method in methods.iter_mut() {
        for (_, type_ann, _, _) in method.params.iter_mut() {
            if let Some(ty) = type_ann.as_mut() {
                *ty = ty.substitute_relative_class_types(self_class, parent);
            }
        }
        if let Some(ret) = method.return_type.as_mut() {
            *ret = ret.substitute_relative_class_types(self_class, parent);
        }
    }
}
