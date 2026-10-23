//! Purpose:
//! Lowers `__elephc_curl_easy_init()` — allocate a libcurl easy handle and hand back the
//! boxed Mixed cell that owns it.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - Takes no operands, so there is nothing to marshal: publish the bridge pointers and
//!   call. The result is already a boxed Mixed (a resource-kind-6 handle, or PHP `false`)
//!   in the integer result register.
//! - THIS IS THE LOWERING THAT MAKES THE FREE PATH REACHABLE. A handle can only enter a
//!   program through here, and this call site publishes `_elephc_curl_easy_free_fn`
//!   along with the rest, so `__rt_mixed_free_deep`'s kind-6 arm can never run against an
//!   unpublished slot.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::shared::ensure_curl_arg_count;

/// Lowers `__elephc_curl_easy_init()` through the easy-handle allocation helper.
pub(crate) fn lower_curl_easy_init(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_init", 0)?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_init");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_easy_from_id($id)` — boxes an id libcurl already created (a
/// server-pushed handle) as a resource-kind-6 Mixed cell. The id is an ordinary integer
/// argument, so the bridge is not called at all; only the free pointer publication is
/// needed, because the resulting cell is released through the shared kind-6 teardown.
pub(crate) fn lower_curl_easy_from_id(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_from_id", 1)?;
    let id = super::super::super::expect_operand(inst, 0)?;
    ctx.load_value_to_result(id)?;
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_from_id");
    store_if_result(ctx, inst)
}
