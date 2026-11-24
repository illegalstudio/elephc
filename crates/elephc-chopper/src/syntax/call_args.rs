//! Purpose:
//! Owns PHP's signature-independent call-argument ordering rules over the AST.
//!
//! Called from:
//! - The parser's dynamic-call validation and the compiler's call-argument planner.
//!
//! Key details:
//! - One implementation serves TOKEN_PARSE and the checker, so ordering fixes cannot drift.
//! - These rules read only call-site expressions; a callee signature is never required.

use crate::syntax::errors::CompileError;
use crate::syntax::parser::ast::{Expr, ExprKind};
use crate::syntax::span::Span;

/// One syntactic argument-ordering violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallArgumentOrderError {
    /// A named argument is followed by an unpack.
    SpreadAfterNamed { span: Span },
    /// A positional argument follows a named argument.
    PositionalAfterNamed { span: Span },
    /// A positional argument follows an unpack.
    PositionalAfterSpread { span: Span },
    /// A statically known unpack supplies a positional entry after a named one.
    PositionalAfterNamedUnpack { span: Span },
}

impl CallArgumentOrderError {
    /// Returns the offending argument's span.
    pub fn span(self) -> Span {
        match self {
            Self::SpreadAfterNamed { span }
            | Self::PositionalAfterNamed { span }
            | Self::PositionalAfterSpread { span }
            | Self::PositionalAfterNamedUnpack { span } => span,
        }
    }
}

/// Validates that no unpack follows a named argument.
pub fn validate_spread_after_named(args: &[Expr]) -> Result<(), CallArgumentOrderError> {
    let mut seen_named = false;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { .. } => seen_named = true,
            ExprKind::Spread(_) if seen_named => {
                return Err(CallArgumentOrderError::SpreadAfterNamed { span: arg.span });
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validates that no positional argument follows an unpack.
pub fn validate_positional_spread_order(args: &[Expr]) -> Result<(), CallArgumentOrderError> {
    let mut seen_spread = false;
    for arg in args {
        if matches!(arg.kind, ExprKind::Spread(_)) {
            seen_spread = true;
        } else if seen_spread && !matches!(arg.kind, ExprKind::NamedArg { .. }) {
            return Err(CallArgumentOrderError::PositionalAfterSpread { span: arg.span });
        }
    }
    Ok(())
}

/// Validates the order of a statically known associative unpack.
pub fn validate_static_unpack_order(args: &[Expr]) -> Result<(), CallArgumentOrderError> {
    for arg in args {
        let ExprKind::Spread(inner) = &arg.kind else {
            continue;
        };
        let ExprKind::ArrayLiteralAssoc(pairs) = &inner.kind else {
            continue;
        };
        let mut seen_named = false;
        for (key, _) in pairs {
            match static_spread_key(key) {
                Some(StaticSpreadKey::Named) => seen_named = true,
                Some(StaticSpreadKey::Positional) if seen_named => {
                    return Err(CallArgumentOrderError::PositionalAfterNamedUnpack {
                        span: arg.span,
                    });
                }
                Some(StaticSpreadKey::Positional) => {}
                // A key that is not a literal says nothing about this array.
                None => break,
            }
        }
    }
    Ok(())
}

/// Validates the ordering PHP rejects while PARSING, without inspecting array contents.
pub fn validate_call_site_argument_order(args: &[Expr]) -> Result<(), CallArgumentOrderError> {
    validate_spread_after_named(args)?;
    validate_positional_spread_order(args)?;
    let mut seen_named = false;
    for arg in args {
        if matches!(arg.kind, ExprKind::NamedArg { .. }) {
            seen_named = true;
        } else if seen_named {
            return Err(CallArgumentOrderError::PositionalAfterNamed { span: arg.span });
        }
    }
    Ok(())
}

/// Validates every signature-independent call-argument rule, including static unpack order.
pub fn validate_unresolved_call_argument_order(
    args: &[Expr],
) -> Result<(), CallArgumentOrderError> {
    validate_call_site_argument_order(args)?;
    validate_static_unpack_order(args)
}

/// Maps one ordering error to the caller-facing diagnostic.
fn call_order_message(error: CallArgumentOrderError, callee_desc: &str) -> CompileError {
    match error {
        CallArgumentOrderError::SpreadAfterNamed { span } => CompileError::new(
            span,
            &format!("{callee_desc} cannot use argument unpacking after named arguments"),
        ),
        CallArgumentOrderError::PositionalAfterNamed { span } => CompileError::new(
            span,
            &format!("{callee_desc} cannot use positional arguments after named arguments"),
        ),
        CallArgumentOrderError::PositionalAfterSpread { span } => CompileError::new(
            span,
            &format!("{callee_desc} cannot use positional arguments after spread arguments"),
        ),
        CallArgumentOrderError::PositionalAfterNamedUnpack { span } => CompileError::new(
            span,
            &format!(
                "{callee_desc} cannot use positional argument after named argument during unpacking"
            ),
        ),
    }
}

/// Maps the parser-only ordering rules to a caller-facing diagnostic.
///
/// TOKEN_PARSE must not reject an unpack because of what a literal array contains; that is a
/// runtime failure in PHP, so only call-site ordering is checked here.
pub fn validate_dynamic_call_argument_order(
    args: &[Expr],
    callee_desc: &str,
) -> Result<(), CompileError> {
    validate_call_site_argument_order(args)
        .map_err(|error| call_order_message(error, callee_desc))
}

/// Validates the full unresolved-call rules and maps them to a caller-facing diagnostic.
pub fn validate_call_argument_order(args: &[Expr], callee_desc: &str) -> Result<(), CompileError> {
    validate_unresolved_call_argument_order(args)
        .map_err(|error| call_order_message(error, callee_desc))
}

/// One static key class in an associative unpack.
enum StaticSpreadKey {
    Positional,
    Named,
}

/// Classifies a literal array key as positional or named for ordering validation.
fn static_spread_key(key: &Expr) -> Option<StaticSpreadKey> {
    match &key.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::FloatLiteral(_) => {
            Some(StaticSpreadKey::Positional)
        }
        ExprKind::StringLiteral(value) if is_php_integer_array_key(value) => {
            Some(StaticSpreadKey::Positional)
        }
        ExprKind::StringLiteral(_) => Some(StaticSpreadKey::Named),
        _ => None,
    }
}

/// Returns true if `value` is a PHP-valid integer array key.
///
/// A valid PHP integer string is a decimal integer that fits in a signed 64-bit integer
/// (`-9223372036854775808` to `9223372036854775807`), with no leading zeros (except `"0"`),
/// no leading `+`, and no whitespace.
pub fn is_php_integer_array_key(value: &str) -> bool {
    if value == "0" {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let (start, negative) = if bytes[0] == b'-' {
        if bytes.len() == 1 || bytes[1] == b'0' {
            return false;
        }
        (1, true)
    } else {
        if bytes[0] == b'0' {
            return false;
        }
        (0, false)
    };
    let digits = &bytes[start..];
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let limit = if negative {
        "9223372036854775808"
    } else {
        "9223372036854775807"
    };
    digits.len() < limit.len() || (digits.len() == limit.len() && digits <= limit.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wraps one expression in an unpack argument.
    fn spread(arg: Expr) -> Expr {
        Expr::new(ExprKind::Spread(Box::new(arg)), Span::dummy())
    }

    /// Wraps one expression in a named argument.
    fn named(name: &str, arg: Expr) -> Expr {
        Expr::new(
            ExprKind::NamedArg {
                name: name.to_string(),
                value: Box::new(arg),
            },
            Span::dummy(),
        )
    }

    /// Verifies the ordering rules report the offending argument's span.
    #[test]
    fn call_argument_order_rejects_each_invalid_shape() {
        assert_eq!(
            validate_unresolved_call_argument_order(&[
                named("a", Expr::var("a")),
                spread(Expr::var("b")),
            ]),
            Err(CallArgumentOrderError::SpreadAfterNamed {
                span: Expr::new(ExprKind::Spread(Box::new(Expr::var("b"))), Span::dummy()).span,
            })
        );
        assert!(validate_unresolved_call_argument_order(&[
            Expr::var("a"),
            named("b", Expr::var("b")),
            Expr::var("c"),
        ])
        .is_err());
    }

    /// TOKEN_PARSE ignores static unpack contents; the checker still rejects the order.
    #[test]
    fn dynamic_call_order_ignores_static_unpack_contents() {
        let key = Expr::new(ExprKind::StringLiteral("a".into()), Span::dummy());
        let value = Expr::new(ExprKind::IntLiteral(1), Span::dummy());
        let positional = Expr::new(ExprKind::IntLiteral(0), Span::dummy());
        let second = Expr::new(ExprKind::IntLiteral(2), Span::dummy());
        let array = Expr::new(
            ExprKind::ArrayLiteralAssoc(vec![(key, value), (positional, second)]),
            Span::dummy(),
        );
        let unpack = Expr::new(ExprKind::Spread(Box::new(array)), Span::dummy());
        assert!(validate_dynamic_call_argument_order(&[unpack.clone()], "Dynamic method call").is_ok());
        assert!(validate_unresolved_call_argument_order(&[unpack]).is_err());
    }

    /// Verifies integer-like string keys stay positional while real names are named.
    #[test]
    fn integer_array_key_detection_matches_php() {
        assert!(is_php_integer_array_key("0"));
        assert!(is_php_integer_array_key("42"));
        assert!(is_php_integer_array_key("-7"));
        assert!(!is_php_integer_array_key("+7"));
        assert!(!is_php_integer_array_key("07"));
        assert!(!is_php_integer_array_key("a"));
        assert!(!is_php_integer_array_key(""));
    }
}
