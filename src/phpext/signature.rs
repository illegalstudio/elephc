//! Purpose:
//! Validates the PHP-visible value subset independently of the public C export ABI.
//!
//! Called from:
//! - `crate::exports::collect_for_php()` after type checking.
//!
//! Key details:
//! - References, objects, resources and variadics have no producer boundary in this phase.

use crate::{errors::CompileError, parser::ast::{Expr, ExprKind}, span::Span, types::{FunctionSig, PhpType}};

/// Rejects signatures for which the adapter cannot make an independent value copy.
pub(crate) fn validate(name: &str, sig: &FunctionSig, span: Span) -> Result<(), CompileError> {
    let fail = |detail: &str| CompileError::new(span, &format!("PHP export '{name}': {detail}"));
    if sig.variadic.is_some() || sig.by_ref_return || sig.ref_params.iter().any(|r| *r) {
        return Err(fail("--emit phpext requires fixed, by-value parameters and returns"));
    }
    if (0..sig.params.len()).any(|i| !sig.declared_params.get(i).copied().unwrap_or(false)) {
        return Err(fail("PHP export parameters require a declared type; use mixed for supported dynamic values"));
    }
    if !sig.declared_return {
        return Err(fail("PHP export returns require a declared type; use mixed for supported dynamic values"));
    }
    for (_, ty) in &sig.params {
        if !input_storage_supported(ty) {
            return Err(fail("typed array parameters are not supported by --emit phpext; use the PHP array type"));
        }
        if !supported(ty) { return Err(fail(&format!("unsupported parameter type {ty:?}"))); }
        parameter_type(ty).map_err(|detail| fail(&detail))?;
    }
    for (i, default) in sig.defaults.iter().enumerate().filter_map(|(i, value)| value.as_ref().map(|value| (i, value))) {
        default_text(Some(default)).map_err(|detail| {
            CompileError::new(default.span, &format!("PHP export '{name}': {detail}"))
        })?;
        let ty = &sig.params[i].1;
        let (base, nullable) = parameter_type(ty).map_err(|detail| fail(&detail))?;
        if matches!(default.kind, ExprKind::Null) && !nullable && !matches!(base, PhpType::Void | PhpType::Mixed) {
            return Err(fail("null defaults require an explicitly nullable parameter type; use ?T"));
        }
        if !default_matches(ty, default) {
            return Err(fail("default value does not match the declared parameter type"));
        }
    }
    if !supported(&sig.return_type) && sig.return_type != PhpType::Never {
        return Err(fail(&format!("unsupported return type {:?}", sig.return_type)));
    }
    Ok(())
}

/// PHP declarations permit integer defaults for float, without general scalar coercion.
fn default_matches(ty: &PhpType, expr: &Expr) -> bool {
    match ty {
        PhpType::Mixed => true,
        PhpType::Int => matches!(expr.kind, ExprKind::IntLiteral(_)),
        PhpType::Float => matches!(expr.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)),
        PhpType::Bool => matches!(expr.kind, ExprKind::BoolLiteral(_)),
        PhpType::False => matches!(expr.kind, ExprKind::BoolLiteral(false)),
        PhpType::Str => matches!(expr.kind, ExprKind::StringLiteral(_)),
        PhpType::Void => matches!(expr.kind, ExprKind::Null),
        PhpType::Array(_) | PhpType::AssocArray { .. } => matches!(&expr.kind, ExprKind::ArrayLiteral(values) if values.is_empty()),
        PhpType::Union(members) => members.iter().any(|member| default_matches(member, expr)),
        _ => false,
    }
}

/// Shares the accepted default forms between source validation and Zend reflection metadata.
pub(super) fn default_text(expr: Option<&Expr>) -> Result<Option<String>, String> {
    let Some(expr) = expr else { return Ok(None); };
    Ok(Some(match &expr.kind {
        ExprKind::IntLiteral(v) => v.to_string(), ExprKind::FloatLiteral(v) => {
            if v.is_nan() { "NAN".into() } else if *v == f64::INFINITY { "INF".into() }
            else if *v == f64::NEG_INFINITY { "-INF".into() } else { format!("{v:?}") }
        },
        ExprKind::BoolLiteral(v) => v.to_string(), ExprKind::Null => "null".into(),
        ExprKind::ArrayLiteral(values) if values.is_empty() => "[]".into(),
        ExprKind::StringLiteral(v) => format!("\"{}\"", crate::string_bytes::literal_bytes(v).iter().map(|b| format!("\\x{b:02x}")).collect::<String>()),
        _ => return Err("PHP extension defaults currently require scalar, null or empty array constants".into()),
    }))
}

/// Shares nullable and array parameter shapes between source validation and Zend generation.
pub(super) fn parameter_type(ty: &PhpType) -> Result<(PhpType, bool), String> {
    if ty.is_php_array() { return Ok((ty.clone(), false)); }
    if let PhpType::Union(members) = ty {
        let non_null = members.iter().filter(|ty| **ty != PhpType::Void).collect::<Vec<_>>();
        if non_null.len() != members.len() {
            if non_null.len() == 1 { return Ok((non_null[0].clone(), true)); }
            let array = PhpType::Union(non_null.into_iter().cloned().collect());
            if array.is_php_array() { return Ok((array, true)); }
        }
        return Err(format!("PHP export parameter union is not yet supported: {ty:?}"));
    }
    Ok((ty.clone(), false))
}

/// Recognizes recursively copyable types; Mixed values are checked again at runtime.
fn supported(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
            | PhpType::Void | PhpType::Mixed => true,
        PhpType::Array(value) => supported(value),
        PhpType::AssocArray { key, value } => supported(key) && supported(value),
        PhpType::Union(members) => members.iter().all(supported),
        _ => false,
    }
}

/// Enumerates core value helpers whose persistent state is entirely inside the native image.
pub(crate) fn runtime_supported(target: crate::ir::RuntimeFnId) -> bool {
    use crate::ir::RuntimeFnId as R;
    matches!(target, R::Count | R::ArrayIsList | R::ArrayKeyExists | R::ArrayKeyFirst
        | R::ArrayKeyLast | R::ArrayKeys | R::ArrayValues | R::ArrayReverse | R::ArraySlice
        | R::ArrayMerge | R::ArrayPop | R::ArrayPush | R::ArrayShift | R::ArrayUnshift
        | R::ArraySum | R::ArrayProduct | R::ArraySearch | R::ArrayChunk | R::ArrayColumn
        | R::ArrayFlip | R::ArrayFill | R::ArrayFillKeys | R::ArrayCombine
        | R::StrRepeat)
}

/// Keeps nullable wrappers from accidentally exposing specialized native array layouts.
fn input_storage_supported(ty: &PhpType) -> bool {
    if ty.is_php_array() { return true; }
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => false,
        PhpType::Union(members) => {
            let non_null = PhpType::Union(members.iter().filter(|ty| **ty != PhpType::Void).cloned().collect());
            non_null.is_php_array() || members.iter().all(input_storage_supported)
        }
        _ => true,
    }
}
