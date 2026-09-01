//! Purpose:
//! Checks language constructs and runtime builtins for EIR AOT eligibility.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Shared call planning handles named arguments and static spreads.

use super::*;

/// Returns true for language-construct calls that can safely lower through EIR AOT.
pub(super) fn eir_construct_call_is_safe<S>(
    name: &str,
    args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    if has_named_args(args)
        || args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    {
        return false;
    }
    match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "isset" => {
            !args.is_empty()
                && args
                    .iter()
                    .all(|arg| eir_isset_probe_is_safe(arg, support, facts, scope_reads))
        }
        "empty" if args.len() == 1 => match &args[0].kind {
            ExprKind::Variable(name) => eir_variable_probe_is_safe(name, facts, scope_reads),
            _ => expr_is_eir_function_safe(&args[0], support, facts, scope_reads),
        },
        _ => false,
    }
}

/// Returns true when an `isset()` operand can lower without evaluating dynamic scope state.
pub(super) fn eir_isset_probe_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Variable(name) => eir_variable_probe_is_safe(name, facts, scope_reads),
        ExprKind::ArrayAccess { .. } => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when a variable probe can use local facts or direct eval read params.
pub(super) fn eir_variable_probe_is_safe(
    name: &str,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool {
    facts.is_assigned(name) || scope_reads.is_some_and(|reads| reads.contains(name))
}

/// Returns true for builtin calls that the normal EIR backend can lower at runtime.
pub(super) fn eir_runtime_builtin_call_is_safe<S>(
    name: &str,
    args: &[Expr],
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    let short_name = php_symbol_key(name.trim_start_matches('\\'));
    let Some(args) = normalize_eir_runtime_builtin_args(&short_name, args) else {
        return false;
    };
    match short_name.as_str() {
        "boolval" if args.len() == 1 => {
            eir_boolval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "array_key_exists" if args.len() == 2 => {
            eir_array_key_exists_args_are_safe(&args[0], &args[1], support, facts, scope_reads)
        }
        "count" if (1..=2).contains(&args.len()) => {
            eir_count_mode_is_default_zero(args.get(1))
                && eir_count_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "floatval" if args.len() == 1 => {
            eir_floatval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "gettype" if args.len() == 1 => {
            eir_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "intval" if args.len() == 1 => {
            eir_intval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_array" if args.len() == 1 => {
            eir_array_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_iterable" if args.len() == 1 => {
            eir_array_like_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_object" if args.len() == 1 => {
            eir_object_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_numeric" | "is_resource" if args.len() == 1 => {
            eir_scalar_cast_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_finite" | "is_infinite" | "is_nan" if args.len() == 1 => {
            eir_float_predicate_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "is_bool" | "is_double" | "is_float" | "is_int" | "is_integer" | "is_long" | "is_null"
        | "is_real" | "is_scalar" | "is_string"
            if args.len() == 1 =>
        {
            eir_type_probe_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "strval" if args.len() == 1 => {
            eir_strval_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        "strlen" if args.len() == 1 => {
            eir_strlen_arg_is_safe(&args[0], support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Normalizes EIR-safe builtin call arguments for eval AOT gating.
///
/// Static spread arrays are expanded through the shared call planner; dynamic
/// spreads that remain after planning stay on the eval bridge fallback.
pub(super) fn normalize_eir_runtime_builtin_args(short_name: &str, args: &[Expr]) -> Option<Vec<Expr>> {
    let has_spread = args
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    if !has_named_args(args) && !has_spread {
        return Some(args.to_vec());
    }
    let sig = builtin_call_sig(short_name)?;
    let call_span = args.first().map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let plan = plan_call_args(&sig, args, call_span, false, false).ok()?;
    if plan.has_spread_args() {
        return None;
    }
    Some(plan.normalized_args())
}

/// Returns true when `array_key_exists()` can lower through EIR without eval bridge state.
pub(super) fn eir_array_key_exists_args_are_safe<S>(
    key: &Expr,
    array: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    if !eir_array_key_exists_static_key_is_safe(key) {
        return false;
    }
    match &array.kind {
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name)) => true,
        ExprKind::ArrayLiteralAssoc(_) => {
            expr_is_eir_static_array_source_safe(array, support, facts, scope_reads)
        }
        ExprKind::ArrayLiteral(_) => {
            !eir_array_key_exists_static_key_needs_assoc_array(key)
                && expr_is_eir_static_array_source_safe(array, support, facts, scope_reads)
        }
        ExprKind::Variable(name) => {
            !eir_array_key_exists_static_key_needs_assoc_array(key) && facts.is_array_local(name)
        }
        _ => false,
    }
}

/// Returns true when the key type has target-aware lowering for mixed array probes.
pub(super) fn eir_array_key_exists_static_key_is_safe(key: &Expr) -> bool {
    match &key.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Null => true,
        ExprKind::FloatLiteral(_) => static_integral_float_array_key_value(key).is_some(),
        ExprKind::Negate(inner) => {
            matches!(inner.kind, ExprKind::IntLiteral(_))
                || static_integral_float_array_key_value(key).is_some()
        }
        _ => false,
    }
}

/// Returns true when the static key only has safe mixed-array semantics for hashes.
///
/// String keys can now probe indexed arrays too: numeric strings normalize to an
/// integer bounds check and non-integer strings return false on indexed arrays.
pub(super) fn eir_array_key_exists_static_key_needs_assoc_array(key: &Expr) -> bool {
    matches!(key.kind, ExprKind::Null)
}

/// Returns true when `count()` uses PHP's default non-recursive mode.
pub(super) fn eir_count_mode_is_default_zero(mode: Option<&Expr>) -> bool {
    match mode {
        None => true,
        Some(expr) => matches!(expr.kind, ExprKind::IntLiteral(0)),
    }
}

/// Returns true when a value can reach `count()` as a concrete EIR array.
pub(super) fn eir_count_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::Variable(name) if scope_reads.is_some_and(|reads| reads.contains(name)) => true,
        _ => expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads),
    }
}

/// Returns true when a value can reach `boolval()` through an EIR-supported scalar path.
pub(super) fn eir_boolval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `floatval()` through an EIR-supported scalar path.
pub(super) fn eir_floatval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `intval()` through an EIR-supported scalar path.
pub(super) fn eir_intval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `gettype()`/`is_*()` through EIR-safe probes.
pub(super) fn eir_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach array-like type probes through safe EIR paths.
pub(super) fn eir_array_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        || eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `is_iterable()` through currently safe EIR paths.
pub(super) fn eir_array_like_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
        || eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach `is_object()` through currently safe EIR paths.
pub(super) fn eir_object_type_probe_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
        || expr_is_eir_static_array_source_safe(expr, support, facts, scope_reads)
}

/// Returns true when a value can reach IEEE float predicates without PHP coercion surprises.
pub(super) fn eir_float_predicate_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) => true,
        ExprKind::Variable(name) => {
            scope_reads.is_some_and(|reads| reads.contains(name))
                || facts.is_int_local(name)
                || facts.is_float_local(name)
        }
        ExprKind::Negate(inner) | ExprKind::ErrorSuppress(inner) => {
            eir_float_predicate_arg_is_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr }
            if matches!(target, CastType::Int | CastType::Float | CastType::Bool) =>
        {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        _ => false,
    }
}

/// Returns true when a value can reach `strval()` through an EIR-supported scalar path.
pub(super) fn eir_strval_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    eir_scalar_cast_arg_is_safe(expr, support, facts, scope_reads)
}

/// Returns true when an expression is scalar-like enough for EIR cast builtins.
pub(super) fn eir_scalar_cast_arg_is_safe<S>(
    expr: &Expr,
    support: &S,
    facts: &EirLocalFacts,
    scope_reads: Option<&BTreeSet<String>>,
) -> bool
where
    S: EirStaticCallSupport,
{
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Variable(name) => {
            scope_reads.is_some_and(|reads| reads.contains(name))
                || (facts.is_assigned(name) && !facts.is_array_local(name))
        }
        ExprKind::Negate(inner) | ExprKind::ErrorSuppress(inner) => {
            eir_scalar_cast_arg_is_safe(inner, support, facts, scope_reads)
        }
        ExprKind::Cast { target, expr }
            if matches!(
                target,
                CastType::Int
                    | CastType::Float
                    | CastType::String
                    | CastType::Bool
                    | CastType::Void
            ) =>
        {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::ArrayAccess { .. } => {
            expr_is_eir_function_safe(expr, support, facts, scope_reads)
        }
        ExprKind::FunctionCall { name, args } => {
            eir_call_user_func_call_is_safe(name.as_str(), args, support, facts, scope_reads)
                || eir_runtime_builtin_call_is_safe(
                    name.as_str(),
                    args,
                    support,
                    facts,
                    scope_reads,
                )
                || fold_static_builtin_int_call(name.as_str().trim_start_matches('\\'), args)
                    .is_some()
                || support.function_supported(name.as_str(), args)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => support.static_method_supported(receiver, method, args),
        _ => false,
    }
}
