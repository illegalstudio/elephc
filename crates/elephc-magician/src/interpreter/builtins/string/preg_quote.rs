//! Purpose:
//! Declarative eval registry entry and implementation for `preg_quote`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Escapes php-src's PCRE metacharacter set byte-for-byte (`#` included since PHP 7.3),
//!   plus the first byte of a non-empty `$delimiter`.
//! - A NUL byte becomes the four bytes `\000`, as in php-src.
//! - The function needs no regex engine, so it works without the optional PCRE2 package.

eval_builtin! {
    contract: "preg_quote",
    area: String,
    direct: PregQuote,
    values: PregQuote,
}

use super::super::super::*;

/// Bytes PHP's `preg_quote()` prefixes with a backslash.
pub(in crate::interpreter) const PREG_QUOTE_ESCAPED: &[u8] = b".\\+*?[^]$(){}=!<>|:-#";

/// Evaluates PHP `preg_quote(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_quote(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_preg_quote_values_result(&evaluated, context, values)
}

/// Dispatches already evaluated `preg_quote()` arguments.
pub(in crate::interpreter) fn eval_preg_quote_values_result(
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (subject, delimiter) = match args {
        [subject] => (*subject, None),
        [subject, delimiter] => (*subject, Some(*delimiter)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let subject = values.string_bytes(subject)?;
    let delimiter = match delimiter {
        Some(delimiter) if !values.is_null(delimiter)? => {
            eval_reject_container_argument(
                "preg_quote",
                2,
                "delimiter",
                "?string",
                delimiter,
                context,
                values,
            )?;
            values.string_bytes(delimiter)?.first().copied()
        }
        _ => None,
    };
    values.string_bytes_value(&eval_preg_quote_bytes(&subject, delimiter))
}

/// Escapes `bytes` as php-src's `preg_quote()` does.
pub(in crate::interpreter) fn eval_preg_quote_bytes(bytes: &[u8], delimiter: Option<u8>) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte == 0 {
            output.extend_from_slice(b"\\000");
            continue;
        }
        if PREG_QUOTE_ESCAPED.contains(&byte) || delimiter == Some(byte) {
            output.push(b'\\');
        }
        output.push(byte);
    }
    output
}
