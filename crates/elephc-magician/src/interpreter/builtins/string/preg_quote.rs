//! Purpose:
//! Declarative eval registry entry and implementation for `preg_quote`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Escapes php-src's `preg_quote` character set verbatim, byte-for-byte, so eval and
//!   compiled output stay identical for binary input.
//! - NUL is the one byte php spells as a four-character sequence, `\000`, rather than as a
//!   backslash-prefixed byte.
//! - Only the delimiter's FIRST byte participates, compared byte-wise: php escapes just the
//!   leading byte of a multi-byte delimiter.

eval_builtin! {
    contract: "preg_quote",
    area: String,
    direct: PregQuote,
    values: PregQuote,
}

use super::super::super::*;

/// Bytes PHP's `preg_quote` prefixes with a backslash.
///
/// Derived by sweeping every byte `0..=255` through `php -n`, not transcribed from the manual.
/// Note what is absent: `/` is escaped only when it is the delimiter, and `"` `'` `,` `;` `@`
/// `~` never are.
const PREG_QUOTE_ESCAPED: &[u8] = b"!#$()*+-.:<=>?[\\]^{|}";

/// Evaluates PHP's `preg_quote(...)` over its subject and optional delimiter.
pub(in crate::interpreter) fn eval_builtin_preg_quote(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, delimiter) = match args {
        [value] => (value, None),
        [value, delimiter] => (value, Some(delimiter)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let value = eval_expr(value, context, scope, values)?;
    let delimiter = match delimiter {
        Some(delimiter) => {
            let delimiter = eval_expr(delimiter, context, scope, values)?;
            // A null delimiter and an empty one behave identically in php, and both reach here
            // as an empty byte string, so no separate presence flag is needed.
            values.string_bytes(delimiter)?.first().copied()
        }
        None => None,
    };
    let bytes = values.string_bytes(value)?;
    values.string_bytes_value(&eval_preg_quote_bytes(&bytes, delimiter))
}

/// Prefixes every PCRE metacharacter in `bytes` with a single backslash, spelling NUL `\000`.
pub(in crate::interpreter) fn eval_preg_quote_bytes(
    bytes: &[u8],
    delimiter: Option<u8>,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        if *byte == 0 {
            output.extend_from_slice(b"\\000");
            continue;
        }
        if PREG_QUOTE_ESCAPED.contains(byte) || delimiter == Some(*byte) {
            output.push(b'\\');
        }
        output.push(*byte);
    }
    output
}

/// Escapes one already-evaluated subject, with the optional already-evaluated delimiter.
pub(in crate::interpreter) fn eval_preg_quote_values(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, delimiter) = match evaluated_args {
        [value] => (*value, None),
        [value, delimiter] => (*value, Some(*delimiter)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let delimiter = match delimiter {
        Some(delimiter) => values.string_bytes(delimiter)?.first().copied(),
        None => None,
    };
    let bytes = values.string_bytes(value)?;
    values.string_bytes_value(&eval_preg_quote_bytes(&bytes, delimiter))
}
