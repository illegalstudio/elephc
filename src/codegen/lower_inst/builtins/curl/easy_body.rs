//! Purpose:
//! Lowers `__elephc_curl_easy_body($handle)` — take the `CURLOPT_RETURNTRANSFER`-captured
//! response body from the handle as an owned PHP string.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The runtime helper performs the take-and-copy in one step, because the bridge's
//!   buffer pointer is only borrowed until the next call on that handle.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_body($handle)` through the captured-body helper.
pub(crate) fn lower_curl_easy_body(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_body", 1)?;
    load_handle_to_first_arg(ctx, inst, 0, "curl_exec")?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_body");
    store_if_result(ctx, inst)
}
