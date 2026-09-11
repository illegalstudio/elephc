//! Purpose:
//! Checks bridge-compatible signatures and formats their PHP type metadata.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Variadic, by-reference, and return-type constraints remain conservative.

use super::*;

/// Returns true when a module function should expose metadata to eval fragments.
pub(super) fn function_has_eval_metadata(function: &Function) -> bool {
    !function.flags.is_main && !function.name.starts_with('_')
}

/// Returns true when eval can dispatch a native function through the generated bridge.
pub(super) fn function_signature_can_bridge_with_eval(function: &Function) -> bool {
    function
        .params
        .iter()
        .all(|param| !param.by_ref || eval_native_function_ref_param_supported(&param.php_type))
}

/// Returns true when a native function by-reference parameter can use eval bridge staging.
pub(super) fn eval_native_function_ref_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Int
            | PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Object(_)
            | PhpType::Str
    )
}

/// Returns true when eval can dispatch a native method through the generated bridge.
pub(super) fn method_signature_can_bridge_with_eval(signature: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(signature)
        && signature
            .params
            .iter()
            .all(|(_, ty)| eval_native_method_param_supported(ty))
        && eval_native_method_return_supported(&signature.return_type)
}

/// Returns true when eval can dispatch a native constructor through the generated bridge.
pub(super) fn constructor_signature_can_bridge_with_eval(signature: &FunctionSig) -> bool {
    eval_signature_ref_params_supported(signature)
        && signature
            .params
            .iter()
            .all(|(_, ty)| eval_native_constructor_param_supported(ty))
}

/// Returns true when one native method argument type fits the eval method bridge.
pub(super) fn eval_native_method_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true when one native constructor argument type fits the eval bridge.
pub(super) fn eval_native_constructor_param_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    )
}

/// Returns true when one native method return type can be boxed back for eval.
pub(super) fn eval_native_method_return_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Void
            | PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
    )
}

/// Returns true when the indexed parameter is the signature's variadic slot.
pub(super) fn signature_param_is_variadic(signature: &FunctionSig, index: usize, param_name: &str) -> bool {
    signature.variadic.as_deref().is_some_and(|variadic| {
        variadic == param_name
            || signature
                .params
                .get(index)
                .is_some_and(|(name, _)| name == variadic)
    })
}

/// Returns generated type specs for declared native callable parameters.
pub(super) fn eval_native_callable_param_type_specs(signature: &FunctionSig) -> Vec<Option<String>> {
    signature
        .params
        .iter()
        .enumerate()
        .map(|(index, (_, php_type))| {
            if !signature
                .declared_params
                .get(index)
                .copied()
                .unwrap_or(false)
            {
                return None;
            }
            signature
                .param_type_exprs
                .get(index)
                .and_then(Option::as_ref)
                .and_then(eval_native_type_expr_spec)
                .or_else(|| eval_native_php_type_spec(php_type, false))
        })
        .collect()
}

/// Returns a generated type spec for a declared native callable return type.
pub(super) fn eval_native_callable_return_type_spec(signature: &FunctionSig) -> Option<String> {
    signature
        .declared_return
        .then(|| eval_native_php_type_spec(&signature.return_type, true))
        .flatten()
}

/// Formats one parsed PHP type expression for eval native metadata registration.
pub(super) fn eval_native_type_expr_spec(type_expr: &TypeExpr) -> Option<String> {
    match type_expr {
        TypeExpr::Int => Some("int".to_string()),
        TypeExpr::Float => Some("float".to_string()),
        TypeExpr::Bool => Some("bool".to_string()),
        TypeExpr::False => Some("false".to_string()),
        TypeExpr::Str => Some("string".to_string()),
        TypeExpr::Void => Some("null".to_string()),
        TypeExpr::Never => None,
        TypeExpr::Iterable => Some("iterable".to_string()),
        TypeExpr::Array(_) => Some("array".to_string()),
        TypeExpr::Ptr(_) | TypeExpr::Buffer(_) => None,
        TypeExpr::Named(name) => Some(name.as_str().to_string()),
        TypeExpr::Nullable(inner) => {
            let inner = eval_native_type_expr_spec(inner)?;
            Some(format!("?{}", inner))
        }
        TypeExpr::Union(members) => eval_native_type_expr_member_specs(members, "|"),
        TypeExpr::Intersection(members) => eval_native_type_expr_member_specs(members, "&"),
    }
}

/// Formats a compound parsed type expression with the requested separator.
pub(super) fn eval_native_type_expr_member_specs(members: &[TypeExpr], separator: &str) -> Option<String> {
    members
        .iter()
        .map(eval_native_type_expr_spec)
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join(separator))
}

/// Formats one checked PHP type for eval native metadata registration.
pub(super) fn eval_native_php_type_spec(php_type: &PhpType, allow_return_atoms: bool) -> Option<String> {
    if php_type.is_php_array() {
        return Some("array".to_string());
    }
    match php_type {
        PhpType::Int => Some("int".to_string()),
        PhpType::Float => Some("float".to_string()),
        PhpType::Str => Some("string".to_string()),
        PhpType::Bool => Some("bool".to_string()),
        PhpType::False => Some("false".to_string()),
        PhpType::Void if allow_return_atoms => Some("void".to_string()),
        PhpType::Void => Some("null".to_string()),
        PhpType::Never if allow_return_atoms => Some("never".to_string()),
        PhpType::Never => None,
        PhpType::Iterable => Some("iterable".to_string()),
        PhpType::Mixed => Some("mixed".to_string()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some("array".to_string()),
        PhpType::Callable => Some("callable".to_string()),
        PhpType::Object(name) if name.is_empty() => Some("object".to_string()),
        PhpType::Object(name) => Some(name.clone()),
        PhpType::Union(members) => eval_native_php_type_member_specs(members),
        PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => None,
    }
}

/// Formats union members from checked PHP types for eval native metadata registration.
pub(super) fn eval_native_php_type_member_specs(members: &[PhpType]) -> Option<String> {
    members
        .iter()
        .map(|member| eval_native_php_type_spec(member, false))
        .collect::<Option<Vec<_>>>()
        .map(|members| members.join("|"))
}
