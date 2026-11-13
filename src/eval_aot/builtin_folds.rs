//! Purpose:
//! Folds pure literal builtins with PHP-compatible scalar semantics.
//!
//! Called from:
//! - The eval AOT facade and sibling analysis modules.
//!
//! Key details:
//! - Array keys, finite numeric folds, and side-effect gates remain explicit.

use super::*;

/// Folds pure static builtin calls whose integer result is fully known at compile time.
pub(crate) fn fold_static_builtin_int_call(
    short_name: &str,
    args: &[Expr],
    php_only: bool,
) -> Option<i64> {
    let ExprKind::IntLiteral(value) = fold_static_builtin_call(short_name, args, php_only)? else {
        return None;
    };
    Some(value)
}

/// Folds pure static builtin calls whose scalar result is fully known at compile time.
pub(super) fn fold_static_builtin_call(
    short_name: &str,
    args: &[Expr],
    php_only: bool,
) -> Option<ExprKind> {
    let name = php_symbol_key(short_name);
    if crate::types::checker::builtins::catalog::php_only_hidden_builtin_for_profile(
        &name, php_only,
    ) {
        return None;
    }
    let normalized_args = normalize_static_builtin_args(&name, args)?;
    let args = normalized_args.as_slice();
    match name.as_str() {
        name if name == "strlen" => fold_strlen(args).map(ExprKind::IntLiteral),
        name if name == "intval" => fold_intval(args).map(ExprKind::IntLiteral),
        name if name == "floatval" => fold_floatval(args).map(ExprKind::FloatLiteral),
        name if name == "strval" => fold_strval(args).map(ExprKind::StringLiteral),
        name if name == "boolval" => fold_boolval(args).map(ExprKind::BoolLiteral),
        name if name == "is_int" || name == "is_integer" || name == "is_long" => {
            fold_type_probe(args, LiteralTypeProbe::Int).map(ExprKind::BoolLiteral)
        }
        name if name == "is_string" => {
            fold_type_probe(args, LiteralTypeProbe::String).map(ExprKind::BoolLiteral)
        }
        name if name == "is_bool" => {
            fold_type_probe(args, LiteralTypeProbe::Bool).map(ExprKind::BoolLiteral)
        }
        name if name == "is_float" || name == "is_double" || name == "is_real" => {
            fold_type_probe(args, LiteralTypeProbe::Float).map(ExprKind::BoolLiteral)
        }
        name if name == "is_null" => {
            fold_type_probe(args, LiteralTypeProbe::Null).map(ExprKind::BoolLiteral)
        }
        name if name == "is_scalar" => fold_is_scalar(args).map(ExprKind::BoolLiteral),
        name if name == "gettype" => fold_gettype(args).map(ExprKind::StringLiteral),
        name if name == "abs" => fold_abs(args).map(ExprKind::IntLiteral),
        name if name == "count" => fold_count(args).map(ExprKind::IntLiteral),
        name if name == "array_key_exists" => {
            fold_array_key_exists(args).map(ExprKind::BoolLiteral)
        }
        name if name == "floor" => fold_floor(args).map(ExprKind::FloatLiteral),
        name if name == "ceil" => fold_ceil(args).map(ExprKind::FloatLiteral),
        name if name == "sqrt" => fold_sqrt(args).map(ExprKind::FloatLiteral),
        name if name == "round" => fold_round(args).map(ExprKind::FloatLiteral),
        name if name == "ord" => fold_ord(args).map(ExprKind::IntLiteral),
        name if name == "chr" => fold_chr(args).map(ExprKind::StringLiteral),
        name if name == "min" => fold_min(args).map(ExprKind::IntLiteral),
        name if name == "max" => fold_max(args).map(ExprKind::IntLiteral),
        name if name == "strtolower" => {
            fold_ascii_case(args, AsciiCaseFold::Lower).map(ExprKind::StringLiteral)
        }
        name if name == "strtoupper" => {
            fold_ascii_case(args, AsciiCaseFold::Upper).map(ExprKind::StringLiteral)
        }
        name if name == "ucfirst" => {
            fold_ascii_first_char_case(args, FirstCharCaseFold::Upper).map(ExprKind::StringLiteral)
        }
        name if name == "lcfirst" => {
            fold_ascii_first_char_case(args, FirstCharCaseFold::Lower).map(ExprKind::StringLiteral)
        }
        name if name == "strrev" => fold_ascii_strrev(args).map(ExprKind::StringLiteral),
        name if name == "substr" => fold_ascii_substr(args).map(ExprKind::StringLiteral),
        name if name == "str_repeat" => fold_ascii_str_repeat(args).map(ExprKind::StringLiteral),
        name if name == "trim" => {
            fold_ascii_default_trim(args, TrimSide::Both).map(ExprKind::StringLiteral)
        }
        name if name == "ltrim" => {
            fold_ascii_default_trim(args, TrimSide::Left).map(ExprKind::StringLiteral)
        }
        name if name == "rtrim" || name == "chop" => {
            fold_ascii_default_trim(args, TrimSide::Right).map(ExprKind::StringLiteral)
        }
        name if name == "str_contains" => {
            fold_ascii_string_predicate(args, StringPredicate::Contains).map(ExprKind::BoolLiteral)
        }
        name if name == "str_starts_with" => {
            fold_ascii_string_predicate(args, StringPredicate::StartsWith)
                .map(ExprKind::BoolLiteral)
        }
        name if name == "str_ends_with" => {
            fold_ascii_string_predicate(args, StringPredicate::EndsWith).map(ExprKind::BoolLiteral)
        }
        _ => None,
    }
}

/// Normalizes named/static-spread builtin arguments before attempting a static fold.
pub(super) fn normalize_static_builtin_args(short_name: &str, args: &[Expr]) -> Option<Vec<Expr>> {
    let sig = builtin_call_sig(short_name)?;
    let call_span = args.first().map(|arg| arg.span).unwrap_or_else(Span::dummy);
    let plan = plan_call_args(&sig, args, call_span, true, false).ok()?;
    Some(plan.normalized_args())
}

/// Folds `strlen("literal")` to an integer result.
pub(super) fn fold_strlen(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    i64::try_from(value.len()).ok()
}

/// Folds `intval()` for literal scalar inputs whose PHP result is unambiguous here.
pub(super) fn fold_intval(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        ExprKind::StringLiteral(value) => value.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Folds `floatval()` for literal scalar inputs whose PHP result is unambiguous here.
pub(super) fn fold_floatval(args: &[Expr]) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    let value = match &args[0].kind {
        ExprKind::IntLiteral(value) => *value as f64,
        ExprKind::FloatLiteral(value) => *value,
        ExprKind::BoolLiteral(value) => f64::from(u8::from(*value)),
        ExprKind::StringLiteral(value) => value.trim().parse::<f64>().ok()?,
        ExprKind::Null => 0.0,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

/// Folds `strval()` for literal scalar inputs with stable PHP string results.
pub(super) fn fold_strval(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(value) => Some(value.to_string()),
        ExprKind::BoolLiteral(true) => Some("1".to_string()),
        ExprKind::BoolLiteral(false) | ExprKind::Null => Some(String::new()),
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}

/// Folds `boolval()` for literal scalar inputs whose PHP truthiness is clear.
pub(super) fn fold_boolval(args: &[Expr]) -> Option<bool> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::StringLiteral(value) => Some(!(value.is_empty() || value == "0")),
        ExprKind::Null => Some(false),
        _ => None,
    }
}

/// Literal scalar type checked by pure `is_*` builtin folds.
enum LiteralTypeProbe {
    Int,
    String,
    Bool,
    Float,
    Null,
}

/// Folds pure `is_*` type probes for literal scalar inputs.
fn fold_type_probe(args: &[Expr], probe: LiteralTypeProbe) -> Option<bool> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(_) => Some(matches!(probe, LiteralTypeProbe::Int)),
        ExprKind::StringLiteral(_) => Some(matches!(probe, LiteralTypeProbe::String)),
        ExprKind::BoolLiteral(_) => Some(matches!(probe, LiteralTypeProbe::Bool)),
        ExprKind::FloatLiteral(_) => Some(matches!(probe, LiteralTypeProbe::Float)),
        ExprKind::Null => Some(matches!(probe, LiteralTypeProbe::Null)),
        _ => None,
    }
}

/// Folds `is_scalar()` for literal scalar and null inputs.
pub(super) fn fold_is_scalar(args: &[Expr]) -> Option<bool> {
    if args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::IntLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::FloatLiteral(_) => Some(true),
        ExprKind::Null => Some(false),
        _ => None,
    }
}

/// Folds `gettype()` for literal scalar and null inputs with stable PHP spellings.
pub(super) fn fold_gettype(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let ty = match &args[0].kind {
        ExprKind::IntLiteral(_) => "integer",
        ExprKind::FloatLiteral(_) => "double",
        ExprKind::StringLiteral(_) => "string",
        ExprKind::BoolLiteral(_) => "boolean",
        ExprKind::Null => "NULL",
        _ => return None,
    };
    Some(ty.to_string())
}

/// Folds `abs()` for integer literals that stay representable as `int`.
pub(super) fn fold_abs(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    const_int_expr(&args[0])?.checked_abs()
}

/// Folds `count()` for static array literals whose element expressions have no side effects.
pub(super) fn fold_count(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    i64::try_from(static_array_key_ids(&args[0])?.len()).ok()
}

/// Folds `array_key_exists()` for static array literals and static scalar keys.
pub(super) fn fold_array_key_exists(args: &[Expr]) -> Option<bool> {
    if args.len() != 2 {
        return None;
    }
    let key = static_array_key_fold_id(&args[0])?;
    Some(static_array_key_ids(&args[1])?.contains(&key))
}

/// Returns normalized key identifiers for a static array literal.
pub(super) fn static_array_key_ids(expr: &Expr) -> Option<BTreeSet<String>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            if !items.iter().all(static_array_value_is_fold_safe) {
                return None;
            }
            (0..items.len())
                .map(|index| Some(format!("i:{index}")))
                .collect()
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            let mut keys = BTreeSet::new();
            for (key, value) in pairs {
                if !static_array_value_is_fold_safe(value) {
                    return None;
                }
                keys.insert(static_array_key_fold_id(key)?);
            }
            Some(keys)
        }
        _ => None,
    }
}

/// Returns true when evaluating this expression while building a static array has no side effects.
pub(super) fn static_array_value_is_fold_safe(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::UnaryPlus(inner) | ExprKind::Negate(inner) | ExprKind::Not(inner) | ExprKind::BitNot(inner) => {
            static_array_value_is_fold_safe(inner)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(static_array_value_is_fold_safe),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().all(|(key, value)| {
            static_array_key_fold_id(key).is_some() && static_array_value_is_fold_safe(value)
        }),
        _ => false,
    }
}

/// Returns a normalized key identifier for static array-literal count folding.
pub(super) fn static_array_key_fold_id(expr: &Expr) -> Option<String> {
    if let Some(value) = static_integer_array_key_value(expr) {
        return Some(format!("i:{value}"));
    }
    match &expr.kind {
        ExprKind::Null => Some("s:".to_string()),
        ExprKind::StringLiteral(value) => Some(format!("s:{value}")),
        _ => None,
    }
}

/// Folds `floor()` for finite numeric literals.
pub(super) fn fold_floor(args: &[Expr]) -> Option<f64> {
    fold_finite_numeric_unary(args, f64::floor)
}

/// Folds `ceil()` for finite numeric literals.
pub(super) fn fold_ceil(args: &[Expr]) -> Option<f64> {
    fold_finite_numeric_unary(args, f64::ceil)
}

/// Folds `sqrt()` for non-negative finite numeric literals.
pub(super) fn fold_sqrt(args: &[Expr]) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    let value = const_finite_numeric_expr(&args[0])?;
    (value >= 0.0)
        .then(|| value.sqrt())
        .filter(|sqrt| sqrt.is_finite())
}

/// Folds one-argument `round()` for finite numeric literals.
pub(super) fn fold_round(args: &[Expr]) -> Option<f64> {
    fold_finite_numeric_unary(args, f64::round)
}

/// Applies a finite `f64` builtin fold to one numeric literal argument.
pub(super) fn fold_finite_numeric_unary(args: &[Expr], fold: fn(f64) -> f64) -> Option<f64> {
    if args.len() != 1 {
        return None;
    }
    let value = fold(const_finite_numeric_expr(&args[0])?);
    value.is_finite().then_some(value)
}

/// Folds `ord()` for literal strings by returning the first byte value.
pub(super) fn fold_ord(args: &[Expr]) -> Option<i64> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(value) = &args[0].kind else {
        return None;
    };
    value.as_bytes().first().map(|byte| i64::from(*byte))
}

/// Folds `chr()` for ASCII byte values representable by the AST string type.
pub(super) fn fold_chr(args: &[Expr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let value = const_int_expr(&args[0])?;
    let byte = u8::try_from(value).ok()?;
    if !byte.is_ascii() {
        return None;
    }
    Some(char::from(byte).to_string())
}

/// Folds `min()` over integer literal arguments.
pub(super) fn fold_min(args: &[Expr]) -> Option<i64> {
    fold_int_values(args)?.into_iter().min()
}

/// Folds `max()` over integer literal arguments.
pub(super) fn fold_max(args: &[Expr]) -> Option<i64> {
    fold_int_values(args)?.into_iter().max()
}

/// Collects integer literal arguments for variadic pure numeric folds.
pub(super) fn fold_int_values(args: &[Expr]) -> Option<Vec<i64>> {
    if args.is_empty() {
        return None;
    }
    args.iter().map(const_int_expr).collect()
}
