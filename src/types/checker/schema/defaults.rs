//! Purpose:
//! Revalidates declaration defaults that depend on complete class-like schemas.
//! Covers class/interface/enum method parameters and deferred object property compatibility.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` after enum schema construction.
//!
//! Key details:
//! - The initial schema pass cannot reliably resolve inheritance or interface relationships.
//! - Direct scoped-constant method defaults and Object-to-Object pairs are revisited.
//! - Plain property scoped-constant defaults stay outside this pass until EIR lowering supports them.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, Program, StaticReceiver, StmtKind};
use crate::types::{traits::FlattenedClass, FunctionSig, PhpType};

use super::super::{infer_expr_type_syntactic, Checker};

/// Validates every declaration default deferred during class-like schema construction.
/// Returns all incompatibilities so the driver can aggregate them with other schema errors.
pub(crate) fn validate_deferred_declaration_defaults(
    checker: &mut Checker,
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
        let Some(interface_info) = checker.interfaces.get(name).cloned() else {
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
            validate_signature_deferred_defaults(
                checker,
                signature,
                "Method",
                Some(name),
                &mut errors,
            );
        }
        for method_key in &interface_info.static_method_order {
            let is_declared_here = interface_info
                .static_method_declaring_interfaces
                .get(method_key)
                .is_some_and(|declaring| php_symbol_key(declaring) == interface_key);
            if !is_declared_here {
                continue;
            }
            let Some(signature) = interface_info.static_methods.get(method_key) else {
                continue;
            };
            validate_signature_deferred_defaults(
                checker,
                signature,
                "Method",
                Some(name),
                &mut errors,
            );
        }
    }

    normalize_method_default_receivers(checker);

    errors
}

/// Rewrites relative receivers in stored method defaults to their declaration scope.
/// Defaults are lowered at call sites, whose active class can differ from the declaring class.
fn normalize_method_default_receivers(checker: &mut Checker) {
    let class_parents: std::collections::HashMap<String, Option<String>> = checker
        .classes
        .iter()
        .map(|(name, info)| (name.clone(), info.parent.clone()))
        .collect();
    let class_names: Vec<String> = checker.classes.keys().cloned().collect();
    for class_name in class_names {
        let Some(class_info) = checker.classes.get_mut(&class_name) else {
            continue;
        };
        let instance_declaring = class_info.method_declaring_classes.clone();
        for (method_key, signature) in &mut class_info.methods {
            let owner = instance_declaring
                .get(method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            let parent = class_parents.get(owner).and_then(Option::as_deref);
            normalize_signature_default_receivers(signature, owner, parent);
        }
        let static_declaring = class_info.static_method_declaring_classes.clone();
        for (method_key, signature) in &mut class_info.static_methods {
            let owner = static_declaring
                .get(method_key)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            let parent = class_parents.get(owner).and_then(Option::as_deref);
            normalize_signature_default_receivers(signature, owner, parent);
        }
    }

    for interface_info in checker.interfaces.values_mut() {
        let instance_declaring = interface_info.method_declaring_interfaces.clone();
        for (method_key, signature) in &mut interface_info.methods {
            let Some(owner) = instance_declaring.get(method_key) else {
                continue;
            };
            normalize_signature_default_receivers(signature, owner, None);
        }
        let static_declaring = interface_info.static_method_declaring_interfaces.clone();
        for (method_key, signature) in &mut interface_info.static_methods {
            let Some(owner) = static_declaring.get(method_key) else {
                continue;
            };
            normalize_signature_default_receivers(signature, owner, None);
        }
    }
}

/// Resolves direct `self::`, `static::`, and `parent::` defaults for one stored signature.
fn normalize_signature_default_receivers(
    signature: &mut FunctionSig,
    owner_class: &str,
    parent_class: Option<&str>,
) {
    for default in &mut signature.defaults {
        let Some(Expr {
            kind: ExprKind::ScopedConstantAccess { receiver, .. },
            ..
        }) = default
        else {
            continue;
        };
        let resolved = match receiver {
            StaticReceiver::Named(_) => None,
            StaticReceiver::Self_ | StaticReceiver::Static => Some(owner_class),
            StaticReceiver::Parent => parent_class,
        };
        if let Some(class_name) = resolved {
            *receiver = StaticReceiver::Named(Name::from(class_name.to_string()));
        }
    }
}

/// Revalidates local method and property defaults for one source-declared class or enum.
fn validate_class_defaults(checker: &mut Checker, class_name: &str, errors: &mut Vec<CompileError>) {
    let Some(class_info) = checker.classes.get(class_name).cloned() else {
        return;
    };
    validate_class_property_defaults(checker, class_name, &class_info, errors);

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
        validate_signature_deferred_defaults(
            checker,
            signature,
            "Method",
            Some(class_name),
            errors,
        );
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

/// Revalidates schema-dependent defaults for declared parameters in one callable signature.
fn validate_signature_deferred_defaults(
    checker: &mut Checker,
    signature: &FunctionSig,
    callable_kind: &str,
    owner_class: Option<&str>,
    errors: &mut Vec<CompileError>,
) {
    let previous_class = checker.current_class.clone();
    checker.current_class = owner_class.map(str::to_string);
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
        validate_deferred_parameter_default(
            checker,
            expected_ty,
            default,
            &format!("{} parameter ${}", callable_kind, param_name),
            errors,
        );
    }
    checker.current_class = previous_class;
}

/// Resolves a direct scoped-constant default semantically, or rechecks a deferred object pair.
fn validate_deferred_parameter_default(
    checker: &mut Checker,
    expected_ty: &PhpType,
    default: &Expr,
    context: &str,
    errors: &mut Vec<CompileError>,
) {
    if matches!(default.kind, ExprKind::ScopedConstantAccess { .. }) {
        if let Err(error) = checker.validate_resolved_declared_default_type(
            expected_ty,
            Some(default),
            default.span,
            context,
        ) {
            errors.extend(error.flatten());
        }
        return;
    }
    validate_object_default(checker, expected_ty, default, context, errors);
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
