//! Purpose:
//! Lowers `__elephc_curl_easy_perform($handle)` — run the handle's configured transfer to
//! completion and report whether libcurl accepted it.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The only operand is the boxed handle, unboxed into the first C argument register.
//! - The answer is a plain `0`/`1`; the specific failure reason is read afterwards
//!   through `curl_errno()` / `curl_error()`, exactly as in PHP.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::{ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_perform($handle)` through the transfer helper.
pub(crate) fn lower_curl_easy_perform(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_perform", 1)?;
    load_handle_to_first_arg(ctx, inst, 0, "curl_exec")?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_perform");
    store_if_result(ctx, inst)
}
