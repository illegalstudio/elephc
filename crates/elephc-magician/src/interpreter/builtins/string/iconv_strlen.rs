//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_strlen()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - An omitted or `null` `$encoding` counts in `iconv.internal_encoding`, while an
//!   explicitly empty one counts in PHP's `default_charset`.

eval_builtin! {
    contract: "iconv_strlen",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_charset, eval_iconv_int};

/// Applies PHP `iconv_strlen(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_strlen_result(
    subject: RuntimeCellHandle,
    encoding: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = values.string_bytes(subject)?;
    let charset = eval_iconv_charset(encoding, values)?;
    let counted = elephc_iconv::strlen(&subject, charset.as_deref());
    eval_iconv_int("iconv_strlen", counted, values)
}
