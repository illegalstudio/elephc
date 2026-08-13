//! Purpose:
//! Lowers `__elephc_curl_easy_error($handle)` — copy libcurl's own message for the
//! handle's most recent transfer into an owned PHP string.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The runtime helper owns the `CURL_ERROR_SIZE` buffer and the `__rt_str_persist`
//!   copy, so this lowering only supplies the handle.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_error($handle)` through the error-message helper.
pub(crate) fn lower_curl_easy_error(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_error", 1)?;
    load_handle_to_first_arg(ctx, inst, 0, "curl_error")?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_error");
    store_if_result(ctx, inst)
}
