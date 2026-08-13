//! Purpose:
//! Dispatches the `ext/curl` group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{Instruction, RuntimeFnId};

/// Lowers a target owned by bounded dispatch group 14, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::CurlEasyBody => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_body(ctx, inst)
        }),
        RuntimeFnId::CurlEasyErrno => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_errno(ctx, inst)
        }),
        RuntimeFnId::CurlEasyError => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_error(ctx, inst)
        }),
        RuntimeFnId::CurlEasyInit => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_init(ctx, inst)
        }),
        RuntimeFnId::CurlEasyPerform => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_perform(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetoptLong => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_setopt_long(ctx, inst)
        }),
        RuntimeFnId::CurlEasySetoptStr => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_easy_setopt_str(ctx, inst)
        }),
        RuntimeFnId::CurlVersion => Some({
            crate::codegen::lower_inst::builtins::curl::lower_curl_version(ctx, inst)
        }),
        _ => None,
    }
}
