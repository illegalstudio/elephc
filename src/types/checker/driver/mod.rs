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
use crate::parser::ast::{Program, StmtKind};
use crate::types::{traits::flatten_classes, TypeEnv};

use super::builtin_types::{
    inject_builtin_reflection, inject_builtin_throwables, patch_builtin_exception_signatures,
    patch_builtin_fiber_signatures, patch_builtin_reflection_signatures,
    patch_magic_method_signatures, InterfaceDeclInfo,
};
use super::builtin_iterators::{inject_builtin_iterators, patch_builtin_generator_signatures};
use super::builtin_json::{inject_builtin_json_interfaces, patch_builtin_json_signatures};
use super::builtin_stdclass::inject_builtin_stdclass;
use super::schema::{
    build_class_info_recursive, build_enum_info, build_interface_info_recursive,
};
use super::yield_validation::validate_yield_contexts;
use super::Checker;

mod externs;
mod functions;
mod init;
mod top_level;

pub(super) fn check_types_impl(
    program: &Program,
    target_platform: Platform,
) -> Result<(Checker, TypeEnv), CompileError> {
    let mut checker = Checker::new(target_platform);
    let mut errors = Vec::new();

    errors.extend(validate_yield_contexts(program));

    checker.collect_function_decls(program, &mut errors);

    let (flattened_classes, flatten_errors) = flatten_classes(program);
    errors.extend(flatten_errors);
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
            interface_map.insert(
                name.clone(),
                InterfaceDeclInfo {
                    name: name.clone(),
                    extends: extends
                        .iter()
                        .map(|name| name.as_str().to_string())
                        .collect(),
                    methods: methods.clone(),
                    span: stmt.span,
                    constants: constants.clone(),
                },
            );
        }
    }
    if let Err(error) = inject_builtin_throwables(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_iterators(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_json_interfaces(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }
    if let Err(error) = inject_builtin_stdclass(&mut class_map) {
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
    for stmt in program {
        if let StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } = &stmt.kind
        {
            if let Err(error) = build_enum_info(
                name,
                backing_type.as_ref(),
                cases,
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
    patch_magic_method_signatures(&mut checker);

    checker.prescan_extern_decls(program, &mut errors);

    let (global_env, initial_top_level_errors) = checker.check_top_level_program(program);

    checker.resolve_unchecked_functions(&mut errors);
    checker.type_check_methods_until_stable(&flattened_classes, &global_env, &mut errors)?;

    let (final_global_env, final_top_level_errors) = checker.check_top_level_program(program);
    for ((stmt, initial_errors), final_errors) in program
        .iter()
        .zip(initial_top_level_errors.into_iter())
        .zip(final_top_level_errors.into_iter())
    {
        if !final_errors.is_empty() {
            errors.extend(final_errors);
            continue;
        }
        if !Checker::can_suppress_initial_top_level_errors(stmt, &initial_errors) {
            errors.extend(initial_errors);
        }
    }

    if !errors.is_empty() {
        return Err(CompileError::from_many(errors));
    }

    Ok((checker, final_global_env))
}
