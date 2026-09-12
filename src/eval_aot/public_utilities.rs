//! Purpose:
//! Exposes constant evaluation and static user-function eligibility helpers.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Argument normalization reuses shared call planning and scalar type checks.
//! - Compiler-generated argument snapshot slots are physical ABI details, not part of the
//!   source signature used to decide whether a static eval call is eligible.

use super::*;

/// Evaluates integer-only literal expressions recognized by eval AOT analysis.
pub(crate) fn const_int_expr(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => const_int_expr(inner)?.checked_neg(),
        _ => None,
    }
}

/// Evaluates finite numeric literal expressions recognized by eval AOT analysis.
pub(super) fn const_finite_numeric_expr(expr: &Expr) -> Option<f64> {
    const MAX_EXACT_F64_INT: i64 = 9_007_199_254_740_992;
    let value = match &expr.kind {
        ExprKind::IntLiteral(value) if (-MAX_EXACT_F64_INT..=MAX_EXACT_F64_INT).contains(value) => {
            *value as f64
        }
        ExprKind::FloatLiteral(value) => *value,
        ExprKind::Negate(inner) => -const_finite_numeric_expr(inner)?,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

/// Checks a user-function signature against the native-only eval call subset.
pub(crate) fn static_function_signature_supported(signature: &FunctionSig, args: &[Expr]) -> bool {
    let visible_regular = crate::types::call_args::regular_param_count(signature);
    let source_variadic = signature
        .variadic
        .as_deref()
        .is_some_and(|name| name != crate::func_args::HIDDEN_ARGS_PARAM);
    if !signature.declared_return
        || signature
            .declared_params
            .iter()
            .take(visible_regular)
            .any(|declared| !declared)
        || signature.ref_params.len() != signature.params.len()
        || source_variadic
        || !static_function_return_type_supported(&signature.return_type)
    {
        return false;
    }
    let Some(args) = normalize_static_function_args(signature, args) else {
        return false;
    };
    visible_regular == args.len()
        && signature
            .params
            .iter()
            .take(visible_regular)
            .zip(signature.ref_params.iter().copied())
            .zip(args.iter())
            .all(|((param, by_ref), arg)| !by_ref && static_function_arg_supported(&param.1, arg))
}

/// Normalizes user-function arguments for eval AOT eligibility checks.
///
/// Static spread arrays are expanded through the shared call planner; dynamic
/// spreads that remain after planning stay on the eval bridge fallback.
pub(super) fn normalize_static_function_args(signature: &FunctionSig, args: &[Expr]) -> Option<Vec<Expr>> {
    if !crate::types::call_args::has_named_args(args)
        && !args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    {
        return normalize_positional_static_function_args(signature, args);
    }
    let call_span = args.first().map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let plan = plan_call_args(signature, args, call_span, false, false).ok()?;
    if plan.has_spread_args() {
        return None;
    }
    Some(plan.normalized_args())
}

/// Appends scalar default values for positional static user-function calls.
pub(super) fn normalize_positional_static_function_args(
    signature: &FunctionSig,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    let visible_regular = crate::types::call_args::regular_param_count(signature);
    if args.len() > visible_regular {
        return None;
    }
    let mut normalized = args.to_vec();
    for idx in args.len()..visible_regular {
        let default = signature.defaults.get(idx)?.clone()?;
        normalized.push(default);
    }
    Some(normalized)
}

/// Returns true when a user function return can be boxed by eval EIR AOT.
pub(super) fn static_function_return_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
    )
}

/// Returns true when a literal argument matches the supported scalar parameter type.
pub(super) fn static_function_arg_supported(param_ty: &PhpType, arg: &Expr) -> bool {
    matches!(
        (param_ty.codegen_repr(), &arg.kind),
        (PhpType::Int, ExprKind::IntLiteral(_))
            | (PhpType::Bool, ExprKind::BoolLiteral(_))
            | (PhpType::Float, ExprKind::FloatLiteral(_))
            | (PhpType::Str, ExprKind::StringLiteral(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a scalar function signature with an optional compiler-generated collector.
    fn scalar_signature(hidden_collector: bool) -> FunctionSig {
        let mut signature = FunctionSig {
            params: vec![("value".to_string(), PhpType::Int)],
            param_type_exprs: vec![None],
            param_attributes: vec![Vec::new()],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
            deprecation: None,
        };
        if hidden_collector {
            signature.params.push((
                crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            ));
            signature.param_type_exprs.push(None);
            signature.param_attributes.push(Vec::new());
            signature.defaults.push(None);
            signature.ref_params.push(false);
            signature.declared_params.push(false);
            signature.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());
        }
        signature
    }

    /// Global backtrace capture must not make an otherwise eligible static eval call fall back.
    #[test]
    fn hidden_argument_collector_does_not_change_static_eval_eligibility() {
        let args = vec![Expr::new(ExprKind::IntLiteral(7), Span::dummy())];
        assert!(static_function_signature_supported(
            &scalar_signature(false),
            &args
        ));
        assert!(static_function_signature_supported(
            &scalar_signature(true),
            &args
        ));
    }
}
