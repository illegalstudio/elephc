//! Purpose:
//! Provides reusable operand conversions and stack-padding helpers for calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Loads an SSA value and moves it into the first integer/pointer argument register.
pub(in crate::codegen) fn load_value_to_first_int_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<PhpType> {
    let ty = ctx.load_value_to_result(value)?;
    move_int_result_to_first_arg(ctx);
    Ok(ty)
}

/// Casts a Mixed source in the first integer arg into one owned string copy.
pub(in crate::codegen) fn emit_mixed_string_for_persistent_store(ctx: &mut FunctionContext<'_>) {
    let non_string = ctx.next_label("mixed_string_persist_non_string");
    let done = ctx.next_label("mixed_string_persist_done");
    let mixed_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_push_reg(ctx.emitter, mixed_arg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // check whether the Mixed payload already holds a string
            ctx.emitter.instruction(&format!("b.ne {}", non_string));           // non-string casts need scratch conversion before persistence
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the generic cast path after the direct string persist
            ctx.emitter.label(&non_string);
            abi::emit_pop_reg(ctx.emitter, mixed_arg);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // check whether the Mixed payload already holds a string
            ctx.emitter.instruction(&format!("jne {}", non_string));            // non-string casts need scratch conversion before persistence
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into str_persist's input register
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the generic cast path after the direct string persist
            ctx.emitter.label(&non_string);
            abi::emit_pop_reg(ctx.emitter, mixed_arg);
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        }
    }
    ctx.emitter.label(&done);
}

/// Resolves `value` into the canonical integer result register, unboxing a boxed `Mixed`/`Union`
/// payload through `__rt_mixed_cast_int`.
///
/// `Int`/`Bool` load directly; every other type is an `unsupported` diagnostic. The `Mixed` path
/// emits a call that clobbers the caller-saved argument registers, so a caller that has already
/// staged other arguments in those registers must spill across this resolution (the integer is left
/// in the int result register on return).
pub(in crate::codegen) fn resolve_int_operand_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                context, ty
            )));
        }
    }
    Ok(())
}

/// Resolves a `?int` argument, answering `NULL_SENTINEL` when the value is null.
///
/// `resolve_int_operand_to_result` flattens a boxed null to `0` — the same answer a legitimate
/// `0` gives — which is wrong for every php parameter whose null means "no bound":
/// `fgets($h, null)` reads the whole line and `fwrite($h, $d, null)` writes every byte, while a
/// `0` raised `Argument #2 ($length) must be greater than 0` and wrote nothing respectively. The
/// value only reaches the builtin as a boxed cell when it arrives through a `mixed` binding —
/// forwarding through an untyped function parameter is the usual route — so the null-typed and
/// literal cases are settled statically and only the boxed one needs the runtime peek.
pub(in crate::codegen) fn resolve_nullable_int_operand_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int_nullable");
            Ok(())
        }
        _ => resolve_int_operand_to_result(ctx, value, context),
    }
}

/// Moves the canonical integer result register into the target's first argument register.
pub(super) fn move_int_result_to_first_arg(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if result_reg == arg_reg {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", arg_reg, result_reg)); // move the loaded value into the runtime helper argument register
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, {}", arg_reg, result_reg)); // move the loaded value into the runtime helper argument register
        }
    }
}

/// Returns the temporary caller-stack pad needed to match incoming stack-arg offsets.
pub(in crate::codegen) fn direct_call_stack_pad_bytes(
    ctx: &FunctionContext<'_>,
    overflow_bytes: usize,
) -> usize {
    abi::outgoing_call_stack_pad_bytes(ctx.emitter.target, overflow_bytes)
}

