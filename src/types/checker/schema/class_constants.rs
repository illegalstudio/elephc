//! Purpose:
//! Validates PHP 8.3 typed class-constant declarations after class-like schemas exist.
//! Checks declared types, initializer values, and covariant inheritance contracts.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` after enum schema construction.
//!
//! Key details:
//! - Validation is deferred so object/interface relationships and scoped constant values resolve.
//! - Precise initializer types are strict apart from PHP's allowed int-to-float widening.
//! - Conservative `Mixed` inference is accepted when an expression cannot be narrowed statically.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{ClassConst, Program, StmtKind, TypeExpr};
use crate::types::checker::builtin_types::InterfaceDeclInfo;
use crate::types::traits::FlattenedClass;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

/// Validates typed constants declared by classes, interfaces, enums, and traits.
/// Returns every independent error so the checker can report the complete schema failure.
pub(crate) fn validate_deferred_class_constants(
    checker: &mut Checker,
    flattened_classes: &[FlattenedClass],
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    flattened_enums: &HashMap<String, FlattenedClass>,
    program: &Program,
) -> Vec<CompileError> {
    let mut errors = Vec::new();

    for class in flattened_classes {
        for constant in &class.constants {
            validate_constant_declaration(checker, &class.name, constant, &mut errors);
            validate_class_constant_contract(checker, class, constant, &mut errors);
        }
    }

    for interface in interface_map.values() {
        for constant in &interface.constants {
            validate_constant_declaration(checker, &interface.name, constant, &mut errors);
            validate_interface_constant_contract(
                checker,
                &interface.name,
                &interface.extends,
                constant,
                &mut errors,
            );
        }
    }

    for enum_unit in flattened_enums.values() {
        for constant in &enum_unit.constants {
            validate_constant_declaration(checker, &enum_unit.name, constant, &mut errors);
            validate_interface_contracts_for_constant(
                checker,
                &enum_unit.name,
                &enum_unit.implements,
                constant,
                &mut errors,
            );
        }
    }

    validate_trait_constants(checker, program, &mut errors);
    errors
}

/// Validates one constant's allowed type and initializer value.
fn validate_constant_declaration(
    checker: &mut Checker,
    owner: &str,
    constant: &ClassConst,
    errors: &mut Vec<CompileError>,
) {
    let Some(type_expr) = &constant.type_expr else {
        return;
    };
    let expected = match resolve_constant_type(checker, owner, constant, type_expr) {
        Ok(expected) => expected,
        Err(error) => {
            errors.extend(error.flatten());
            return;
        }
    };
    let value = constant_value(checker, owner, &constant.name)
        .unwrap_or_else(|| constant.value.clone());
    let previous_class = checker.current_class.replace(owner.to_string());
    let actual = checker.infer_type(&value, &TypeEnv::default());
    checker.current_class = previous_class;
    match actual {
        Ok(actual) if strict_type_accepts(checker, &expected, &actual, true) => {}
        Ok(actual) => errors.push(CompileError::new(
            constant.span,
            &format!(
                "Cannot use {} as value for class constant {}::{} of type {}",
                actual, owner, constant.name, expected
            ),
        )),
        Err(error) => errors.extend(error.flatten()),
    }
}

/// Resolves a declared constant type after rejecting PHP-forbidden type atoms.
fn resolve_constant_type(
    checker: &Checker,
    owner: &str,
    constant: &ClassConst,
    type_expr: &TypeExpr,
) -> Result<PhpType, CompileError> {
    if let Some(forbidden) = forbidden_constant_type(type_expr) {
        return Err(CompileError::new(
            constant.span,
            &format!(
                "Class constant {}::{} cannot have type {}",
                owner, constant.name, forbidden
            ),
        ));
    }
    checker.resolve_type_expr(type_expr, constant.span)
}

/// Returns the first `void`, `never`, or `callable` atom forbidden by PHP for constants.
fn forbidden_constant_type(type_expr: &TypeExpr) -> Option<&'static str> {
    match type_expr {
        TypeExpr::Void => Some("void"),
        TypeExpr::Never => Some("never"),
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "void" => Some("void"),
            "never" => Some("never"),
            "callable" => Some("callable"),
            _ => None,
        },
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            forbidden_constant_type(inner)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().find_map(forbidden_constant_type)
        }
        _ => None,
    }
}

/// Validates one direct class constant against parent-class and interface declarations.
fn validate_class_constant_contract(
    checker: &Checker,
    class: &FlattenedClass,
    constant: &ClassConst,
    errors: &mut Vec<CompileError>,
) {
    if let Some(parent_name) = &class.extends {
        if let Some((declaring, parent_type)) =
            lookup_class_constant_contract(checker, parent_name, &constant.name)
        {
            validate_override_type(checker, &class.name, constant, &declaring, parent_type, errors);
        }
    }
    validate_interface_contracts_for_constant(
        checker,
        &class.name,
        &class.implements,
        constant,
        errors,
    );
}

/// Validates one direct interface constant against every directly extended parent contract.
fn validate_interface_constant_contract(
    checker: &Checker,
    interface_name: &str,
    parents: &[String],
    constant: &ClassConst,
    errors: &mut Vec<CompileError>,
) {
    for parent_name in parents {
        let Some(parent_info) = checker.interfaces.get(parent_name) else {
            continue;
        };
        if !parent_info.constants.contains_key(&constant.name) {
            continue;
        }
        let declaring = parent_info
            .constant_declaring_interfaces
            .get(&constant.name)
            .map(String::as_str)
            .unwrap_or(parent_name);
        validate_override_type(
            checker,
            interface_name,
            constant,
            declaring,
            parent_info.constant_types.get(&constant.name),
            errors,
        );
    }
}

/// Validates a class-like constant against matching constants on implemented interfaces.
fn validate_interface_contracts_for_constant(
    checker: &Checker,
    owner: &str,
    interfaces: &[String],
    constant: &ClassConst,
    errors: &mut Vec<CompileError>,
) {
    for interface_name in interfaces {
        let Some(interface_info) = checker.interfaces.get(interface_name) else {
            continue;
        };
        if !interface_info.constants.contains_key(&constant.name) {
            continue;
        }
        let declaring = interface_info
            .constant_declaring_interfaces
            .get(&constant.name)
            .map(String::as_str)
            .unwrap_or(interface_name);
        validate_override_type(
            checker,
            owner,
            constant,
            declaring,
            interface_info.constant_types.get(&constant.name),
            errors,
        );
    }
}

/// Validates covariance against one inherited typed constant contract.
fn validate_override_type(
    checker: &Checker,
    owner: &str,
    constant: &ClassConst,
    inherited_owner: &str,
    inherited_type_expr: Option<&TypeExpr>,
    errors: &mut Vec<CompileError>,
) {
    let Some(inherited_type_expr) = inherited_type_expr else {
        return;
    };
    let inherited = match checker.resolve_type_expr(inherited_type_expr, constant.span) {
        Ok(inherited) => inherited,
        Err(error) => {
            errors.extend(error.flatten());
            return;
        }
    };
    let Some(type_expr) = &constant.type_expr else {
        errors.push(incompatible_override_error(
            owner,
            constant,
            inherited_owner,
            &inherited,
        ));
        return;
    };
    let child = match resolve_constant_type(checker, owner, constant, type_expr) {
        Ok(child) => child,
        Err(_) => return,
    };
    if !strict_type_accepts(checker, &inherited, &child, false) {
        errors.push(incompatible_override_error(
            owner,
            constant,
            inherited_owner,
            &inherited,
        ));
    }
}

/// Builds the PHP-compatible diagnostic for a non-covariant constant override.
fn incompatible_override_error(
    owner: &str,
    constant: &ClassConst,
    inherited_owner: &str,
    inherited: &PhpType,
) -> CompileError {
    CompileError::new(
        constant.span,
        &format!(
            "Type of {}::{} must be compatible with {}::{} of type {}",
            owner, constant.name, inherited_owner, constant.name, inherited
        ),
    )
}

/// Finds the nearest class declaration of a constant and its optional declared type.
fn lookup_class_constant_contract<'a>(
    checker: &'a Checker,
    class_name: &str,
    constant_name: &str,
) -> Option<(String, Option<&'a TypeExpr>)> {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        let info = checker.classes.get(&name)?;
        if info.constants.contains_key(constant_name) {
            return Some((name, info.constant_types.get(constant_name)));
        }
        current = info.parent.clone();
    }
    None
}

/// Returns the schema-normalized value expression for one class-like constant.
fn constant_value(
    checker: &Checker,
    owner: &str,
    constant_name: &str,
) -> Option<crate::parser::ast::Expr> {
    checker
        .classes
        .get(owner)
        .and_then(|info| info.constants.get(constant_name))
        .or_else(|| {
            checker
                .interfaces
                .get(owner)
                .and_then(|info| info.constants.get(constant_name))
        })
        .cloned()
}

/// Returns whether `expected` accepts every value represented by `actual` without coercion.
/// `allow_int_to_float` is enabled only for initializer values, never for variance checks.
fn strict_type_accepts(
    checker: &Checker,
    expected: &PhpType,
    actual: &PhpType,
    allow_int_to_float: bool,
) -> bool {
    if expected == actual || matches!(expected, PhpType::Mixed) {
        return true;
    }
    if allow_int_to_float && matches!(actual, PhpType::Mixed) {
        // Expression inference intentionally uses Mixed for operations whose exact
        // compile-time value is narrower (for example `self::INT_CONST * 3`).
        return true;
    }
    match (expected, actual) {
        (PhpType::Bool, PhpType::False) => true,
        (PhpType::Float, PhpType::Int) => allow_int_to_float,
        (PhpType::Union(expected_members), PhpType::Union(actual_members)) => actual_members
            .iter()
            .all(|actual_member| {
                expected_members.iter().any(|expected_member| {
                    strict_type_accepts(checker, expected_member, actual_member, allow_int_to_float)
                })
            }),
        (PhpType::Union(expected_members), _) => expected_members.iter().any(|expected_member| {
            strict_type_accepts(checker, expected_member, actual, allow_int_to_float)
        }),
        (_, PhpType::Union(actual_members)) => actual_members.iter().all(|actual_member| {
            strict_type_accepts(checker, expected, actual_member, allow_int_to_float)
        }),
        (PhpType::Object(_), PhpType::Object(_))
        | (PhpType::Iterable, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable)
        | (PhpType::Iterable, PhpType::Object(_))
        | (PhpType::Array(_), PhpType::Array(_) | PhpType::AssocArray { .. })
        | (PhpType::AssocArray { .. }, PhpType::Array(_) | PhpType::AssocArray { .. }) => {
            checker.type_accepts(expected, actual)
        }
        _ => false,
    }
}

/// Validates directly declared trait constants that do not depend on a relative class type.
fn validate_trait_constants(
    checker: &mut Checker,
    program: &Program,
    errors: &mut Vec<CompileError>,
) {
    for statement in program {
        match &statement.kind {
            StmtKind::TraitDecl {
                name, constants, ..
            } => {
                for constant in constants {
                    if constant
                        .type_expr
                        .as_ref()
                        .is_some_and(type_expr_contains_relative_class)
                    {
                        continue;
                    }
                    validate_constant_declaration(checker, name, constant, errors);
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                validate_trait_constants(checker, body, errors);
            }
            _ => {}
        }
    }
}

/// Returns whether a type expression contains `self`, `static`, or `parent`.
fn type_expr_contains_relative_class(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Named(name) => matches!(
            name.as_str().to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        ),
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            type_expr_contains_relative_class(inner)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_expr_contains_relative_class)
        }
        _ => false,
    }
}
