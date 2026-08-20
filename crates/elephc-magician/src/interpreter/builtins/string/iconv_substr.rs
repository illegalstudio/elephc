//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_substr()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - `$offset` and `$length` count characters, and follow PHP's `substr()` conventions
//!   for negative values and an omitted length.

eval_builtin! {
    contract: "iconv_substr",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_bytes, eval_iconv_charset};

/// Applies PHP `iconv_substr(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_substr_result(
    subject: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    encoding: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = values.string_bytes(subject)?;
    let offset = eval_int_value(offset, values)?;
    let length = match length {
        Some(length) if !values.is_null(length)? => Some(eval_int_value(length, values)?),
        _ => None,
    };
    let charset = eval_iconv_charset(encoding, values)?;
    let sliced = elephc_iconv::substr(&subject, offset, length, charset.as_deref());
    eval_iconv_bytes("iconv_substr", sliced, values)
}
