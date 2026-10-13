//! Purpose:
//! Lowers EIR Parallel submission into the native job/executor bridge ABI.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `Op::ParallelSpawn`.
//!
//! Key details:
//! - PHP serialization bytes are copied by the bridge before the parent string is released.
//! - Callback and runtime-context addresses are process code pointers, never PHP arena pointers.
//! - The seventh SysV argument is stack-passed in a 16-byte aligned outgoing area.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction};
use crate::names::function_symbol;

use super::{expect_operand, store_if_result};

/// Creates and submits one native job, returning its monotonic id or zero on submission failure.
pub(super) fn lower_parallel_spawn(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let serialized = expect_operand(inst, 0)?;
    let callback = match inst.immediate {
        Some(Immediate::Data(data)) => ctx.function_name_data(data)?.to_string(),
        _ => {
            return Err(CodegenIrError::invalid_module(
                "parallel_spawn requires a worker callback name",
            ))
        }
    };
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(serialized, ptr_reg, len_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if ptr_reg != "x0" {
                ctx.emitter.instruction(&format!("mov x0, {ptr_reg}"));         // pass the exact serialized byte pointer to the native bridge
            }
            if len_reg != "x1" {
                ctx.emitter.instruction(&format!("mov x1, {len_reg}"));         // pass the exact serialized byte length to the native bridge
            }
        }
        Arch::X86_64 => {
            if ptr_reg != "rdi" {
                ctx.emitter.instruction(&format!("mov rdi, {ptr_reg}"));        // pass the exact serialized byte pointer to the native bridge
            }
            if len_reg != "rsi" {
                ctx.emitter.instruction(&format!("mov rsi, {len_reg}"));        // pass the exact serialized byte length to the native bridge
            }
        }
    }
    let create = ctx
        .emitter
        .target
        .extern_symbol("elephc_parallel_job_create_php_serialized");
    abi::emit_call_label(ctx.emitter, &create);
    emit_submit(ctx, &callback);
    store_if_result(ctx, inst)
}

/// Submits the saved job id with context lifecycle callbacks and target-aware stack sizing.
fn emit_submit(ctx: &mut FunctionContext<'_>, callback: &str) {
    let submit = ctx
        .emitter
        .target
        .extern_symbol("elephc_parallel_job_submit_v1");
    let callback = function_symbol(callback);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_reserve_temporary_stack(ctx.emitter, 16);
            abi::emit_store_to_sp(ctx.emitter, "x0", 0);
            abi::emit_symbol_address(ctx.emitter, "x1", "__rt_ctx_acquire");
            abi::emit_symbol_address(ctx.emitter, "x2", "__rt_ctx_release");
            abi::emit_symbol_address(ctx.emitter, "x3", "__rt_parallel_worker_entry");
            abi::emit_symbol_address(ctx.emitter, "x4", &callback);
            abi::emit_load_symbol_to_reg(ctx.emitter, "x5", "_heap_max", 0);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x6",
                elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES as i64,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            abi::emit_call_label(ctx.emitter, &submit);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            emit_keep_id_only_if_accepted(ctx, "x9");
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        Arch::X86_64 => {
            abi::emit_reserve_temporary_stack(ctx.emitter, 16);
            abi::emit_store_to_sp(ctx.emitter, "rax", 8);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the newly created monotonic job id
            abi::emit_symbol_address(ctx.emitter, "rsi", "__rt_ctx_acquire");
            abi::emit_symbol_address(ctx.emitter, "rdx", "__rt_ctx_release");
            abi::emit_symbol_address(ctx.emitter, "rcx", "__rt_parallel_worker_entry");
            abi::emit_symbol_address(ctx.emitter, "r8", &callback);
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_heap_max", 0);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rax",
                elephc_parallel_contract::WORKER_STACK_DEFAULT_BYTES as i64,
            );
            abi::emit_store_to_sp(ctx.emitter, "rax", 0);
            abi::emit_call_label(ctx.emitter, &submit);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 8);
            emit_keep_id_only_if_accepted(ctx, "r10");
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
    }
}

/// Returns the saved job id only for an accepted submission; every failure becomes zero.
fn emit_keep_id_only_if_accepted(ctx: &mut FunctionContext<'_>, id_reg: &str) {
    let accepted = ctx.next_label("parallel_submit_accepted");
    let done = ctx.next_label("parallel_submit_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp w0, #1");                              // require the executor's accepted status
            ctx.emitter.instruction(&format!("b.eq {accepted}"));               // return the saved id only for an admitted or queued job
            ctx.emitter.instruction("mov x0, #0");                              // expose submission failure as the invalid zero job id
            ctx.emitter.instruction(&format!("b {done}"));                      // skip the accepted-id restore on failure
            ctx.emitter.label(&accepted);
            ctx.emitter.instruction(&format!("mov x0, {id_reg}"));              // restore the monotonic job id as the EIR result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp eax, 1");                              // require the executor's accepted status
            ctx.emitter.instruction(&format!("je {accepted}"));                 // return the saved id only for an admitted or queued job
            ctx.emitter.instruction("xor eax, eax");                            // expose submission failure as the invalid zero job id
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip the accepted-id restore on failure
            ctx.emitter.label(&accepted);
            ctx.emitter.instruction(&format!("mov rax, {id_reg}"));             // restore the monotonic job id as the EIR result
        }
    }
    ctx.emitter.label(&done);
}
