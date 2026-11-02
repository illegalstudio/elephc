//! Purpose:
//! Retires owned allow-list strings while constructing strip_tags tag delimiters.
//!
//! Called from:
//! - The native strip_tags array allow-list lowerer.
//!
//! Key details:
//! - Every concat result is persisted before retiring its independent input.
//! - The input owner remains visible if concat or persistence unwinds.

use super::*;

/// Concatenates one delimiter with an owned result, consuming that input owner.
pub(super) fn emit_owned_allow_concat(
    ctx: &mut FunctionContext<'_>,
    delimiter: &[u8],
    prefix: bool,
) {
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
    let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_temporary_stack_address(ctx.emitter, owner_address, 0);
    abi::emit_push_call_operand_owner(ctx.emitter, owner_address, false);
    let (label, len) = ctx.data.add_string(delimiter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if prefix {
                ctx.emitter.instruction("mov x3, x1");                          // pass the owned input after the opening delimiter
                ctx.emitter.instruction("mov x4, x2");                          // preserve the owned input length as concat's right operand
                abi::emit_symbol_address(ctx.emitter, "x1", &label);
                abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            } else {
                abi::emit_symbol_address(ctx.emitter, "x3", &label);
                abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            }
        }
        Arch::X86_64 => {
            if prefix {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the owned input after the opening delimiter
                ctx.emitter.instruction("mov rsi, rdx");                        // preserve the owned input length as concat's right operand
                abi::emit_symbol_address(ctx.emitter, "rax", &label);
                abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            } else {
                abi::emit_symbol_address(ctx.emitter, "rdi", &label);
                abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_pop_call_operand_owner(ctx.emitter);
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}
