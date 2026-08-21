//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_mime_decode()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - Only the first header field is decoded, because a line break that no linear
//!   whitespace follows ends the field.

eval_builtin! {
    contract: "iconv_mime_decode",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_bytes, eval_iconv_charset};

/// Applies PHP `iconv_mime_decode(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_mime_decode_result(
    subject: RuntimeCellHandle,
    mode: Option<RuntimeCellHandle>,
    encoding: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = values.string_bytes(subject)?;
    let mode = match mode {
        Some(mode) if !values.is_null(mode)? => eval_int_value(mode, values)?,
        _ => 0,
    };
    let charset = eval_iconv_charset(encoding, values)?;
    let decoded = elephc_iconv::mime_decode(&subject, mode, charset.as_deref());
    eval_iconv_bytes("iconv_mime_decode", decoded, values)
}
