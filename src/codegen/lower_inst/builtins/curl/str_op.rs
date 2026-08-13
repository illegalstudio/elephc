//! Purpose:
//! Lowers `__elephc_curl_easy_str_op($handle, $op, $text, $number)` — the widest curl
//! marshalling in the compiler, at five C arguments.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - THE STAGING ORDER IS THE WHOLE FILE, for the two hazards `easy_setopt.rs` documents
//!   at length: unboxing the handle calls `__rt_mixed_unbox`, which clobbers every
//!   caller-saved register, and loading operands straight into argument registers is a
//!   parallel move that can read a register an earlier load already overwrote. Everything
//!   is therefore staged on the stack and popped into place after the handle is unboxed.
//! - THE STRING IS MATERIALIZED FIRST, before anything else occupies a caller-saved
//!   register, because materializing a `Str` operand may itself call a runtime helper.
//! - PUSH ORDER IS THE REVERSE OF POP ORDER, so the four pops below land in argument
//!   order `op`, `number`, `len`, `ptr`. Reading the two lists together is the only way
//!   to check this file, which is why they sit next to each other.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::store_if_result;
use super::super::strings::load_string_arg_to_regs;
use super::shared::{curl_arg_reg, ensure_curl_arg_count, load_handle_to_first_arg};

/// Lowers `__elephc_curl_easy_str_op($handle, $op, $text, $number)`.
pub(crate) fn lower_curl_easy_str_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_curl_arg_count(inst, "__elephc_curl_easy_str_op", 4)?;
    let op = super::super::super::expect_operand(inst, 1)?;
    let number = super::super::super::expect_operand(inst, 3)?;

    let ptr_scratch = abi::int_result_reg(ctx.emitter);
    let len_scratch = abi::secondary_scratch_reg(ctx.emitter);
    load_string_arg_to_regs(
        ctx,
        inst,
        2,
        "__elephc_curl_easy_str_op",
        ptr_scratch,
        len_scratch,
    )?;
    abi::emit_push_reg(ctx.emitter, ptr_scratch);
    abi::emit_push_reg(ctx.emitter, len_scratch);
    ctx.load_value_to_reg(number, ptr_scratch)?;
    abi::emit_push_reg(ctx.emitter, ptr_scratch);
    ctx.load_value_to_reg(op, ptr_scratch)?;
    abi::emit_push_reg(ctx.emitter, ptr_scratch);

    load_handle_to_first_arg(ctx, inst, 0, "curl_getinfo")?;
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 1)); // C ABI op = which operation to run
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 4)); // C ABI number = its integer argument
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 3)); // C ABI len = its string argument's length
    abi::emit_pop_reg(ctx.emitter, curl_arg_reg(ctx, 2)); // C ABI ptr = its string argument's bytes

    crate::codegen::curl::publish_elephc_curl_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_curl_easy_str_op");
    store_if_result(ctx, inst)
}
