//! Purpose:
//! Lowers the internal `__elephc_sodium_box` / `__elephc_sodium_status` builtins behind the
//! `sodium_prelude` sealed-box wrappers.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - The operation code is always an integer literal written by the prelude.
//! - The sodium bridge entry is published at the call site so only programs that use the
//!   sealed-box surface link `-lelephc_crypto`.

use super::*;

/// Lowers `__elephc_sodium_box(op, first, second)` through `__rt_sodium_box`.
pub(crate) fn lower_sodium_box(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_sodium_box", 3)?;
    let op = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 1, "__elephc_sodium_box", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the first string while loading the second
            load_string_arg_to_regs(ctx, inst, 2, "__elephc_sodium_box", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the second string while loading the operation
            ctx.load_value_to_result(op)?;
            ctx.emitter.instruction("ldp x3, x4, [sp], #16");                   // restore the second string into its helper registers
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the first string into its helper registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 1, "__elephc_sodium_box", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 2, "__elephc_sodium_box", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.load_value_to_result(op)?;
            ctx.emitter.instruction("mov r10, rax");                            // pass the operation code to the sodium helper
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    crate::codegen::hash_crypto::publish_elephc_sodium_function_pointers(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_sodium_box");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_sodium_status()` to a load of the status the last sodium call recorded.
pub(crate) fn lower_sodium_status(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_sodium_status", 0)?;
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result, "_elephc_sodium_status", 0);
    store_if_result(ctx, inst)
}
