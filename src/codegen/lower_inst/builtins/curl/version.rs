//! Purpose:
//! Lowers `__elephc_curl_version()` — read the linked libcurl's version info as the JSON
//! blob the prelude decodes into PHP's `curl_version()` array.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - Takes no operands and needs no handle: `curl_version()` describes the LIBRARY, not a
//!   transfer, so the bridge answers it without consulting the handle table.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::ensure_curl_arg_count;

/// Lowers `__elephc_curl_version()` through the version-info helper.
pub(crate) fn lower_curl_version(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_version", 0)?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_version");
    store_if_result(ctx, inst)
}
