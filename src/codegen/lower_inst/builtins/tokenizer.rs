//! Purpose:
//! Lowers the tokenizer's private ParseError location operation on every target.
//!
//! Called from:
//! - The typed runtime-function dispatcher.
//!
//! Key details:
//! - Operands remain borrowed and the result is a plain integer.

use super::*;

/// Stamps the scanned line while preserving the receiver across argument loading.
pub(crate) fn lower_error_location(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let line = expect_operand(inst, 1)?;
    ctx.load_value_to_result(line)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    super::object_props::emit_object_pointer_from_operand(ctx, expect_operand(inst, 0)?)?;
    let line_reg = match ctx.emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rcx" };
    abi::emit_pop_reg(ctx.emitter, line_reg);
    abi::emit_call_label(ctx.emitter, "__rt_tokenizer_error_location");
    store_if_result(ctx, inst)
}
