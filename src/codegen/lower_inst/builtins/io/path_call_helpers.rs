//! Purpose:
//! Shared unary and binary path/runtime call helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Loads a path string into runtime argument/result registers and stores the boolean result.
pub(super) fn lower_unary_path_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads a path string into runtime argument/result registers and stores the integer result.
pub(super) fn lower_unary_path_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads a stream resource, calls a boolean fd runtime helper, and stores its result.
pub(super) fn lower_unary_stream_bool_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Stores `__rt_flock`'s would-block output into a local slot while preserving the return value.
pub(super) fn store_flock_would_block(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x1");                              // move would_block into the canonical integer register for local storage
            abi::store_at_offset(ctx.emitter, "x0", offset);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, rdx");                            // move would_block into the canonical integer register for local storage
            abi::store_at_offset(ctx.emitter, "rax", offset);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Returns the local slot loaded by a stream builtin operand when it came from `load_local`.
pub(super) fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Loads two path strings into the runtime ABI, calls a helper, and stores its result.
pub(super) fn lower_binary_path_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    emit_binary_path_call(ctx, inst, name, runtime_label)
}

/// The same, for a builtin whose third parameter is php's `$context`.
///
/// `$context` is accepted and IGNORED, the way `unlink()`/`mkdir()`/`rmdir()` already accept it:
/// elephc has no context plumbing on this route, and refusing the argument outright was worse —
/// it made `copy($a, $b, $ctx)` a compile error on a signature php documents.
pub(super) fn lower_binary_path_call_with_context(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, name, 2, 3)?;
    emit_binary_path_call(ctx, inst, name, runtime_label)
}

/// Emits the two-path call itself, once the arity has been checked.
fn emit_binary_path_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, first, name)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, second, name)?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the second path pointer in the runtime helper's secondary string slot
            ctx.emitter.instruction("mov x4, x2");                              // pass the second path length in the runtime helper's secondary string slot
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, first, name)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, second, name)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the second path pointer while the first path remains on the stack
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the second path length while the first path remains on the stack
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}
