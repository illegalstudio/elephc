//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - This group owns the iconv extension family, which shares one staged argument block.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 14, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::Iconv => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv(ctx, inst)
        }),
        RuntimeFnId::IconvGetEncoding => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_get_encoding(ctx, inst)
        }),
        RuntimeFnId::IconvMimeDecode => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_mime_decode(ctx, inst)
        }),
        RuntimeFnId::IconvMimeDecodeHeaders => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_mime_decode_headers(ctx, inst)
        }),
        RuntimeFnId::IconvMimeEncode => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_mime_encode(ctx, inst)
        }),
        RuntimeFnId::IconvSetEncoding => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_set_encoding(ctx, inst)
        }),
        RuntimeFnId::IconvStrlen => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_strlen(ctx, inst)
        }),
        RuntimeFnId::IconvStrpos => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_strpos(ctx, inst)
        }),
        RuntimeFnId::IconvStrrpos => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_strrpos(ctx, inst)
        }),
        RuntimeFnId::IconvSubstr => Some({
            crate::codegen::lower_inst::builtins::iconv::lower_iconv_substr(ctx, inst)
        }),
        _ => None,
    }
}
