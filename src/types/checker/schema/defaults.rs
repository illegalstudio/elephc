//! Purpose:
//! Revalidates object-typed declaration defaults after class-like schemas are complete.
//! Covers class/interface methods and declared instance/static property defaults.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` after enum schema construction.
//!
//! Key details:
//! - The initial schema pass cannot reliably resolve inheritance or interface relationships.
//! - Only deferred Object-to-Object pairs are revisited; scalar checks stay in the initial pass.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, Program, StmtKind};
use crate::types::{traits::FlattenedClass, FunctionSig, PhpType};

use super::super::{infer_expr_type_syntactic, Checker};

/// Validates every object-typed default deferred during class-like schema construction.
/// Returns all incompatibilities so the driver can aggregate them with other schema errors.
pub(crate) fn validate_deferred_object_defaults(
    checker: &Checker,
    flattened_classes: &[FlattenedClass],
    program: &Program,
) -> Vec<CompileError> {
    let mut errors = Vec::new();

    for class in flattened_classes {
        validate_class_defaults(checker, &class.name, &mut errors);
    }
    for stmt in program {
        if let StmtKind::EnumDecl { name, .. } = &stmt.kind {
            validate_class_defaults(checker, name, &mut errors);
        }
    }

    for stmt in program {
        let StmtKind::InterfaceDecl { name, .. } = &stmt.kind else {
            continue;
        };
        let Some(interface_info) = checker.interfaces.get(name) else {
            continue;
        };
        let interface_key = php_symbol_key(name);
        for method_key in &interface_info.method_order {
            let is_declared_here = interface_info
                .method_declaring_interfaces
                .get(method_key)
                .is_some_and(|declaring| php_symbol_key(declaring) == interface_key);
            if !is_declared_here {
                continue;
            }
            let Some(signature) = interface_info.methods.get(method_key) else {
                continue;
            };
            validate_signature_object_defaults(checker, signature, "Method", &mut errors);
        }
    }

    errors
}

/// Revalidates local method and property defaults for one source-declared class or enum.
fn validate_class_defaults(checker: &Checker, class_name: &str, errors: &mut Vec<CompileError>) {
    let Some(class_info) = checker.classes.get(class_name) else {
        return;
    };
    validate_class_property_defaults(checker, class_name, class_info, errors);

    for method in &class_info.method_decls {
        let method_key = php_symbol_key(&method.name);
        let signature = if method.is_static {
            class_info.static_methods.get(&method_key)
        } else {
            class_info.methods.get(&method_key)
        };
        let Some(signature) = signature else {
            continue;
        };
        validate_signature_object_defaults(checker, signature, "Method", errors);
    }
}

/// Revalidates local declared instance and static property defaults for one class.
fn validate_class_property_defaults(
    checker: &Checker,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    errors: &mut Vec<CompileError>,
) {
    for (index, (property_name, expected_ty)) in class_info.properties.iter().enumerate() {
        let is_local_declared_property = class_info.declared_properties.contains(property_name)
            && class_info
                .property_declaring_classes
                .get(property_name)
                .is_some_and(|declaring| declaring == class_name);
        if !is_local_declared_property {
            continue;
        }
        let Some(default) = class_info.defaults.get(index).and_then(Option::as_ref) else {
            continue;
        };
        validate_object_default(
            checker,
            expected_ty,
            default,
            &format!("Property {}::${} default", class_name, property_name),
            errors,
        );
    }

    for (index, (property_name, expected_ty)) in class_info.static_properties.iter().enumerate() {
        let is_local_declared_property = class_info
            .declared_static_properties
            .contains(property_name)
            && class_info
                .static_property_declaring_classes
                .get(property_name)
                .is_some_and(|declaring| declaring == class_name);
        if !is_local_declared_property {
            continue;
        }
        let Some(default) = class_info
            .static_defaults
            .get(index)
            .and_then(Option::as_ref)
        else {
            continue;
        };
        validate_object_default(
            checker,
            expected_ty,
            default,
            &format!("Static property {}::${} default", class_name, property_name),
            errors,
        );
    }
}

/// Revalidates object defaults for declared parameters in one resolved callable signature.
fn validate_signature_object_defaults(
    checker: &Checker,
    signature: &FunctionSig,
    callable_kind: &str,
    errors: &mut Vec<CompileError>,
) {
    for (index, ((param_name, expected_ty), default)) in signature
        .params
        .iter()
        .zip(signature.defaults.iter())
        .enumerate()
    {
        if !signature
            .declared_params
            .get(index)
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        let Some(default) = default.as_ref() else {
            continue;
        };
        validate_object_default(
            checker,
            expected_ty,
            default,
            &format!("{} parameter ${}", callable_kind, param_name),
            errors,
        );
    }
}

/// Checks one deferred default when both its declared and syntactic types are objects.
fn validate_object_default(
    checker: &Checker,
    expected_ty: &PhpType,
    default: &Expr,
    context: &str,
    errors: &mut Vec<CompileError>,
) {
    let default_ty = infer_expr_type_syntactic(default);
    if !matches!(expected_ty, PhpType::Object(_)) || !matches!(default_ty, PhpType::Object(_)) {
        return;
    }
    if let Err(error) =
        checker.require_compatible_arg_type(expected_ty, &default_ty, default.span, context)
    {
        errors.extend(error.flatten());
    }
}
