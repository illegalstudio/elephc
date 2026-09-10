//! Purpose:
//! Constant-folds `literal |> fn(...)` pipes when the right-hand callable is a
//! first-class callable referencing a pure, PHP-equivalent built-in we can
//! evaluate at compile time.
//!
//! Called from:
//! - `crate::optimize::fold::expr::fold_expr` (Pipe branch).
//!
//! Key details:
//! - Only Function targets are folded; Method/StaticMethod targets may depend on
//!   runtime receiver context. Conversions and edge cases (i64::MIN abs, non-ASCII
//!   string transforms) are rejected so the fallback runtime call keeps PHP
//!   semantics. The whitelist is intentionally narrow.
//! - String-producing calls fold only when PHP preserves or selects interned identity.

use crate::parser::ast::{CallableTarget, Expr, ExprKind};
use crate::string_bytes;

/// Attempts to constant-fold a `literal |> fn(...)` pipe expression when the
/// right-hand side is a first-class callable to a pure, built-in function whose
/// result depends only on the literal value.
///
/// Returns `Some(ExprKind)` with the folded constant result, or `None` if the
/// pipe cannot be folded at compile time (e.g., the callable is not a
/// `Function` target, or the value is outside the safe-folding domain such as
/// NaN floats, non-ASCII strings, or overflow cases like `i64::MIN.abs()`).
///
/// Only `CallableTarget::Function` names are considered; `Method` and
/// `StaticMethod` targets are returned as `None` because their behavior depends
/// on the receiver at runtime.
///
/// Folded operations include:
/// - `strlen` on string literals
/// - `intval`/`floatval` on int/float literals
/// - `abs` on int/float literals (with overflow check for `i64::MIN`)
/// - `floor`/`ceil`/`round` on float/int literals (NaN/infinity skipped)
/// - `is_int`, `is_float`, `is_string`, `is_bool`, `is_null`, `is_array`
/// - `is_numeric` on int/float/bool/null literals
/// - `gettype` on all literal types
/// - Unchanged `strtoupper`/`strtolower`/`ucfirst`/`lcfirst` on ASCII literals
/// - `trim` when unchanged or reduced to PHP's canonical empty string
///
/// # Arguments
/// * `value` - The left-hand side literal expression being piped
/// * `callable` - The right-hand side callable expression
pub(super) fn try_fold_pure_pipe(value: &Expr, callable: &Expr) -> Option<ExprKind> {
    let target = match &callable.kind {
        ExprKind::FirstClassCallable(target) => target,
        _ => return None,
    };
    let name = match target {
        CallableTarget::Function(name) => name.as_str(),
        _ => return None,
    };
    match (name, &value.kind) {
        // -- length / arithmetic conversions ----------------------------------
        ("strlen", ExprKind::StringLiteral(s)) => {
            Some(ExprKind::IntLiteral(string_bytes::literal_byte_len(s) as i64))
        }
        ("intval", ExprKind::IntLiteral(n)) => Some(ExprKind::IntLiteral(*n)),
        ("intval", ExprKind::FloatLiteral(f)) if f.is_finite() => {
            Some(ExprKind::IntLiteral(*f as i64))
        }
        ("floatval", ExprKind::IntLiteral(n)) => Some(ExprKind::FloatLiteral(*n as f64)),
        ("floatval", ExprKind::FloatLiteral(f)) => Some(ExprKind::FloatLiteral(*f)),
        ("abs", ExprKind::IntLiteral(n)) => n.checked_abs().map(ExprKind::IntLiteral),
        ("abs", ExprKind::FloatLiteral(f)) => Some(ExprKind::FloatLiteral(f.abs())),

        // -- floor/ceil/round on finite floats. PHP returns `float`, matching
        //    Rust's `f.floor()` / `f.ceil()` / `f.round()` semantics. Skip NaN
        //    and infinities to stay on the safe side.
        ("floor", ExprKind::FloatLiteral(f)) if f.is_finite() => {
            Some(ExprKind::FloatLiteral(f.floor()))
        }
        ("ceil", ExprKind::FloatLiteral(f)) if f.is_finite() => {
            Some(ExprKind::FloatLiteral(f.ceil()))
        }
        ("round", ExprKind::FloatLiteral(f)) if f.is_finite() => {
            Some(ExprKind::FloatLiteral(f.round()))
        }
        // floor/ceil/round on an int literal: PHP coerces to float and returns
        // the same value; no-op semantically but normalises the AST type.
        ("floor" | "ceil" | "round", ExprKind::IntLiteral(n)) => {
            Some(ExprKind::FloatLiteral(*n as f64))
        }

        // -- type predicates on literals --------------------------------------
        ("is_int" | "is_integer" | "is_long", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::IntLiteral(_)))),
        ("is_float" | "is_double" | "is_real", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::FloatLiteral(_)))),
        ("is_string", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::StringLiteral(_)))),
        ("is_bool", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::BoolLiteral(_)))),
        ("is_null", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::Null))),
        // `is_array`/`is_object`/`is_scalar` fold only when the piped value is a
        // recognized constant literal. A non-literal (e.g. a variable) must NOT
        // fold — its runtime kind is unknown, so we fall through and let codegen
        // emit the runtime check rather than guessing the result.
        ("is_array", ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) => {
            Some(ExprKind::BoolLiteral(true))
        }
        (
            "is_array",
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null,
        ) => Some(ExprKind::BoolLiteral(false)),
        (
            "is_object",
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::ArrayLiteral(_)
            | ExprKind::ArrayLiteralAssoc(_),
        ) => Some(ExprKind::BoolLiteral(false)),
        (
            "is_scalar",
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_),
        ) => Some(ExprKind::BoolLiteral(true)),
        (
            "is_scalar",
            ExprKind::Null | ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_),
        ) => Some(ExprKind::BoolLiteral(false)),
        // PHP `is_numeric` accepts ints, floats, and numeric strings. Reject
        // strings we cannot reliably classify here; the runtime fallback
        // handles them with the canonical parser.
        ("is_numeric", ExprKind::IntLiteral(_)) => Some(ExprKind::BoolLiteral(true)),
        ("is_numeric", ExprKind::FloatLiteral(_)) => Some(ExprKind::BoolLiteral(true)),
        ("is_numeric", ExprKind::BoolLiteral(_)) => Some(ExprKind::BoolLiteral(false)),
        ("is_numeric", ExprKind::Null) => Some(ExprKind::BoolLiteral(false)),

        // -- `gettype` returns PHP's canonical type-name strings --------------
        ("gettype", ExprKind::IntLiteral(_)) => {
            Some(ExprKind::StringLiteral("integer".to_string()))
        }
        ("gettype", ExprKind::FloatLiteral(_)) => {
            Some(ExprKind::StringLiteral("double".to_string()))
        }
        ("gettype", ExprKind::StringLiteral(_)) => {
            Some(ExprKind::StringLiteral("string".to_string()))
        }
        ("gettype", ExprKind::BoolLiteral(_)) => {
            Some(ExprKind::StringLiteral("boolean".to_string()))
        }
        ("gettype", ExprKind::Null) => Some(ExprKind::StringLiteral("NULL".to_string())),
        ("gettype", ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) => {
            Some(ExprKind::StringLiteral("array".to_string()))
        }

        // -- ASCII string transforms ------------------------------------------
        ("strtoupper", ExprKind::StringLiteral(s)) if s.is_ascii() && !s.bytes().any(|byte| byte.is_ascii_lowercase()) => {
            Some(ExprKind::StringLiteral(s.clone()))
        }
        ("strtolower", ExprKind::StringLiteral(s)) if s.is_ascii() && !s.bytes().any(|byte| byte.is_ascii_uppercase()) => {
            Some(ExprKind::StringLiteral(s.clone()))
        }
        ("ucfirst", ExprKind::StringLiteral(s)) if s.is_ascii() && !s.as_bytes().first().is_some_and(u8::is_ascii_lowercase) => {
            Some(ExprKind::StringLiteral(s.clone()))
        }
        ("lcfirst", ExprKind::StringLiteral(s)) if s.is_ascii() && !s.as_bytes().first().is_some_and(u8::is_ascii_uppercase) => {
            Some(ExprKind::StringLiteral(s.clone()))
        }
        // `trim` with no second argument strips PHP's default whitespace set:
        // Space, tab, newline, carriage return, NUL, and vertical tab.
        ("trim", ExprKind::StringLiteral(s)) => {
            let trimmed: String = s
                .trim_matches(|c: char| {
                    matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B')
                })
                .to_string();
            (trimmed.is_empty() || trimmed == *s).then_some(ExprKind::StringLiteral(trimmed))
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Builds a direct first-class builtin pipe without involving parsing or runtime byte conversion.
    fn folded(name: &str, value: &str) -> Option<ExprKind> {
        let callable = Expr::new(ExprKind::FirstClassCallable(CallableTarget::Function(name.into())), Span::dummy());
        try_fold_pure_pipe(&Expr::string_lit(value), &callable)
    }

    /// Keeps calls that produce fresh strings executable even when every output byte is known.
    #[test]
    fn string_identity_pipe_folds_keep_fresh_results_at_runtime() {
        for (name, value) in [("strtolower", "ASCII"), ("strtoupper", "ascii"),
            ("ucfirst", "ascii"), ("lcfirst", "ASCII"), ("strrev", "ASCII"),
            ("strrev", "aba"), ("strrev", "a"), ("strrev", ""), ("trim", " ASCII ")] {
            assert_eq!(folded(name, value), None, "{name}({value:?}) must retain PHP fresh-string identity");
        }
    }

    /// Retains safe literal and canonical-empty folds while preserving PHP's form-feed trim behavior.
    #[test]
    fn string_identity_pipe_folds_preserve_interned_results() {
        for (name, value, expected) in [("strtolower", "ascii", "ascii"), ("strtoupper", "ASCII", "ASCII"),
            ("ucfirst", "ASCII", "ASCII"), ("lcfirst", "ascii", "ascii"), ("strtolower", "", ""),
            ("trim", "ASCII", "ASCII"), ("trim", " \t\n\r\0\u{b}", ""), ("trim", "\u{c}ASCII\u{c}", "\u{c}ASCII\u{c}")] {
            assert_eq!(folded(name, value), Some(ExprKind::StringLiteral(expected.to_owned())), "{name}({value:?})");
        }
    }
}
