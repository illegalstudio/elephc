//! Purpose:
//! Home of the PHP `iconv_mime_decode_headers` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The result is a boxed associative array whose values are strings, or lists of strings
//!   when one field name repeats, so the checker contract is `mixed`.
//! - A failure anywhere in the block discards the whole result and yields `false`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_mime_decode_headers",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvMimeDecodeHeaders,
    ),
}

/// Validates `iconv_mime_decode_headers()`'s arguments and returns `PhpType::Mixed`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_mime_decode_headers", "headers")?;
    if let Some(mode) = cx.args.get(1) {
        cx.checker.infer_type(mode, cx.env)?;
    }
    super::iconv_strlen::check_nullable_string_argument(
        cx,
        2,
        "iconv_mime_decode_headers",
        "encoding",
    )?;
    Ok(PhpType::Mixed)
}
