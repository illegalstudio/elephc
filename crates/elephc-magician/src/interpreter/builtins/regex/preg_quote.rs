//! Purpose:
//! Eval registry entry and implementation for `preg_quote`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Mirrors the AOT prelude (`crate::pcre_prelude` in the compiler): each regex metacharacter
//!   and the delimiter's first byte get a backslash, and NUL becomes `\000`, like php-src.
//! - A null or empty delimiter escapes nothing extra.

use super::super::super::*;

eval_builtin! {
    contract: "preg_quote",
    area: Regex,
    direct: PregQuote,
    values: PregQuote,
}

/// The bytes php-src escapes besides NUL and the delimiter.
const PREG_QUOTE_SPECIALS: &[u8] = b".\\+*?[^]$(){}=!<>|:-#";

/// Evaluates PHP `preg_quote()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_quote(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated = args
        .iter()
        .map(|arg| eval_expr(arg, context, scope, values))
        .collect::<Result<Vec<_>, _>>()?;
    eval_preg_quote_values_result(&evaluated, values)
}

/// Dispatches by-value `preg_quote()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_quote_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (subject, delimiter) = match evaluated_args {
        [subject] => (*subject, None),
        [subject, delimiter] => (*subject, Some(*delimiter)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let delimiter = match delimiter {
        Some(delimiter) if !values.is_null(delimiter)? => {
            values.string_bytes(delimiter)?.first().copied()
        }
        _ => None,
    };
    let bytes = values.string_bytes(subject)?;
    values.string_bytes_value(&preg_quote_bytes(&bytes, delimiter))
}

/// Escapes `bytes` with php-src's `preg_quote()` rules.
fn preg_quote_bytes(bytes: &[u8], delimiter: Option<u8>) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte == 0 {
            output.extend_from_slice(b"\\000");
        } else if PREG_QUOTE_SPECIALS.contains(&byte) || Some(byte) == delimiter {
            output.push(b'\\');
            output.push(byte);
        } else {
            output.push(byte);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::preg_quote_bytes;

    /// Verifies the metacharacters, the delimiter and NUL are escaped like php-src.
    #[test]
    fn test_preg_quote_bytes_matches_php() {
        assert_eq!(preg_quote_bytes(b"a.b*c", None), b"a\\.b\\*c");
        assert_eq!(preg_quote_bytes(b"a/b#c", Some(b'/')), b"a\\/b\\#c");
        assert_eq!(preg_quote_bytes(b"a/b", None), b"a/b");
        assert_eq!(preg_quote_bytes(b"x\0y", None), b"x\\000y");
    }
}
