//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `iconv_strrpos()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - PHP's signature has no `$offset`, so the whole haystack is always scanned.

eval_builtin! {
    contract: "iconv_strrpos",
    area: String,
    direct: Iconv,
    values: Iconv,
}

use super::super::super::*;
use super::iconv::{eval_iconv_charset, eval_iconv_search};

/// Applies PHP `iconv_strrpos(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_iconv_strrpos_result(
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    encoding: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let charset = eval_iconv_charset(encoding, values)?;
    let found = elephc_iconv::strrpos(&haystack, &needle, charset.as_deref());
    eval_iconv_search("iconv_strrpos", found, context, values)
}
