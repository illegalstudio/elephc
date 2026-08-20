//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_strpos()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - The reported position counts characters; an empty `$needle` never matches.
//! - An `$offset` outside `$haystack` raises PHP's catchable `ValueError`.

eval_builtin! {
    contract: "iconv_strpos",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_charset, eval_iconv_search};

/// Applies PHP `iconv_strpos(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_strpos_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    offset: Option<RuntimeCellHandle>,
    encoding: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let offset = match offset {
        Some(offset) if !values.is_null(offset)? => eval_int_value(offset, values)?,
        _ => 0,
    };
    let charset = eval_iconv_charset(encoding, values)?;
    let found = elephc_iconv::strpos(&haystack, &needle, offset, charset.as_deref());
    eval_iconv_search("iconv_strpos", found, context, values)
}
