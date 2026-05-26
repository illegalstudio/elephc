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
/// - `strtoupper`/`strtolower`/`strrev`/`ucfirst`/`lcfirst` on ASCII strings
/// - `trim` on string literals (using PHP's default whitespace set)
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
        ("is_int", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::IntLiteral(_)))),
        ("is_float", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::FloatLiteral(_)))),
        ("is_string", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::StringLiteral(_)))),
        ("is_bool", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::BoolLiteral(_)))),
        ("is_null", value_kind) => Some(ExprKind::BoolLiteral(matches!(value_kind, ExprKind::Null))),
        ("is_array", value_kind) => Some(ExprKind::BoolLiteral(matches!(
            value_kind,
            ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
        ))),
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
        ("strtoupper", ExprKind::StringLiteral(s)) if s.is_ascii() => {
            Some(ExprKind::StringLiteral(s.to_ascii_uppercase()))
        }
        ("strtolower", ExprKind::StringLiteral(s)) if s.is_ascii() => {
            Some(ExprKind::StringLiteral(s.to_ascii_lowercase()))
        }
        ("strrev", ExprKind::StringLiteral(s)) if s.is_ascii() => {
            let reversed: String = s.bytes().rev().map(char::from).collect();
            Some(ExprKind::StringLiteral(reversed))
        }
        ("ucfirst", ExprKind::StringLiteral(s)) if s.is_ascii() => {
            let mut out = s.clone();
            if let Some(first) = out.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            Some(ExprKind::StringLiteral(out))
        }
        ("lcfirst", ExprKind::StringLiteral(s)) if s.is_ascii() => {
            let mut out = s.clone();
            if let Some(first) = out.get_mut(0..1) {
                first.make_ascii_lowercase();
            }
            Some(ExprKind::StringLiteral(out))
        }
        // `trim` with no second argument strips PHP's default whitespace set:
        // " \t\n\r\0\x0B".
        ("trim", ExprKind::StringLiteral(s)) => {
            let trimmed: String = s
                .trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B'))
                .to_string();
            Some(ExprKind::StringLiteral(trimmed))
        }

        _ => None,
    }
}
