//! Purpose:
//! Lowers `__elephc_curl_easy_errno($handle)` — report the `CURLcode` from the handle's
//! most recent transfer.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The runtime helper SIGN-extends this one's `int32_t`, unlike its boolean siblings,
//!   because a `CURLcode` is a signed enum value rather than a flag.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_errno($handle)` through the error-code helper.
pub(crate) fn lower_curl_easy_errno(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_errno", 1)?;
    load_handle_to_first_arg(ctx, inst, 0, "curl_errno")?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_errno");
    store_if_result(ctx, inst)
}
