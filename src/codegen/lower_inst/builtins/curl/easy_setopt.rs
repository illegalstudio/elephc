//! Purpose:
//! Lowers the two `curl_setopt()` entry points — `__elephc_curl_easy_setopt_long($handle,
//! $option, $value)` and `__elephc_curl_easy_setopt_str($handle, $option, $value)` — which
//! differ only in how the third operand is materialized.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - MARSHALLING ORDER IS THE WHOLE PROBLEM HERE, and it is why both lowerings stage
//!   their operands on the stack instead of loading straight into the C argument
//!   registers. Two independent hazards make the direct form wrong:
//!   1. Unboxing the handle CALLS `__rt_mixed_unbox`, which clobbers every caller-saved
//!      register — including any argument register already loaded. `hash_update` solves
//!      the same problem the same way (`emit_push_reg` around the second operand).
//!   2. Loading operand A into argument register R1 and operand B into R2 is a PARALLEL
//!      MOVE: if B's home register happens to be R1, the second load reads a register the
//!      first load already overwrote. Staging through the stack makes the moves
//!      sequential and removes the hazard regardless of what the register allocator chose.
//! - The two lowerings deliberately share `emit_setopt_call` so the staging order can only
//!   ever be changed for both at once.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::super::strings::load_string_arg_to_regs;
use super::shared::{curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_setopt_long($handle, $option, $value)`.
pub(crate) fn lower_curl_easy_setopt_long(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_setopt_long", 3)?;
    let option = super::super::super::expect_operand(inst, 1)?;
    let value = super::super::super::expect_operand(inst, 2)?;

    // Stage value then option, so the pops below restore them in argument order.
    let scratch = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(value, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);
    ctx.load_value_to_reg(option, scratch)?;
    abi::emit_push_reg(ctx.emitter, scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_setopt")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1));                       // C ABI opt = the libcurl option number
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 2));                       // C ABI value = the long value

    emit_setopt_call(ctx, "__rt_curl_easy_setopt_long");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_curl_easy_setopt_str($handle, $option, $value)`.
///
/// The string operand becomes the C ABI's third and fourth arguments (pointer and length),
/// so three values are staged rather than two.
pub(crate) fn lower_curl_easy_setopt_str(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_setopt_str", 3)?;
    let option = super::super::super::expect_operand(inst, 1)?;

    // The string materialization may itself call a runtime helper (a Mixed operand is
    // converted before it can be handed to C), so it happens FIRST, before anything else
    // occupies a caller-saved register.
    let ptr_scratch = abi::int_result_reg(ctx.emitter);
    let len_scratch = abi::secondary_scratch_reg(ctx.emitter);
    load_string_arg_to_regs(
        ctx,
        inst,
        2,
        "__elephc_curl_easy_setopt_str",
        ptr_scratch,
        len_scratch,
    )?;
    abi::emit_push_reg(ctx.emitter, len_scratch);
    abi::emit_push_reg(ctx.emitter, ptr_scratch);
    ctx.load_value_to_reg(option, ptr_scratch)?;
    abi::emit_push_reg(ctx.emitter, ptr_scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_setopt")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1));                       // C ABI opt = the libcurl option number
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 2));                       // C ABI ptr = the value's bytes
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 3));                       // C ABI len = the value's byte length

    emit_setopt_call(ctx, "__rt_curl_easy_setopt_str");
    store_if_result(ctx, inst)
}

/// Publishes the bridge pointers and calls one of the two setopt runtime helpers.
fn emit_setopt_call(ctx: &mut FunctionContext<'_>, runtime_label: &str) {
    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, runtime_label);
}
