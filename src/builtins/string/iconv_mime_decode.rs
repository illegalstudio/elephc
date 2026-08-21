//! Purpose:
//! Home of the PHP `iconv_mime_decode` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Only the first header field is decoded, because a line break that no linear whitespace
//!   follows ends the field.
//! - `$mode` accepts `ICONV_MIME_DECODE_STRICT` and `ICONV_MIME_DECODE_CONTINUE_ON_ERROR`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "iconv_mime_decode",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::IconvMimeDecode,
    ),
}

/// Validates `iconv_mime_decode()`'s arguments and returns `PhpType::Union([Str, False])`.
///
/// The hook infers every argument itself so a container passed where PHP declares a
/// string is rejected here instead of reaching the backend.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::iconv_strlen::check_string_argument(cx, 0, "iconv_mime_decode", "string")?;
    if let Some(mode) = cx.args.get(1) {
        cx.checker.infer_type(mode, cx.env)?;
    }
    super::iconv_strlen::check_nullable_string_argument(cx, 2, "iconv_mime_decode", "encoding")?;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
