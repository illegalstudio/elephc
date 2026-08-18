//! Purpose:
//! Lowers PCNTL signal masks and synchronous waits through stable bridge buffers.
//!
//! Called from:
//! - `super::pcntl::lower()` for typed signal-mask and signal-wait EIR operations.
//!
//! Key details:
//! - Signal arrays use contiguous widened integer payloads on every supported target.
//! - By-reference outputs are replaced only after a successful OS operation.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed, CodegenIrError,
    Result,
};
use crate::ir::{Instruction, LocalSlotId};
use crate::types::PhpType;

use super::pcntl::{
    pcntl_output_local_slot, pcntl_siginfo_output_local_slot, store_pcntl_siginfo_array,
};
use super::strings::load_as_int;
use super::{ensure_arg_count_between, expect_operand, store_if_result};

const SIGNAL_CAPACITY: usize = 128;
const SIGNAL_BUFFER_BYTES: usize = SIGNAL_CAPACITY * std::mem::size_of::<i64>();
const MASK_MODE_OFFSET: usize = SIGNAL_BUFFER_BYTES;
const MASK_RESULT_OFFSET: usize = SIGNAL_BUFFER_BYTES + 8;
const MASK_FRAME_BYTES: usize = SIGNAL_BUFFER_BYTES + 32;
const SIGINFO_BYTES: usize = 96;
const WAIT_SIGNALS_OFFSET: usize = SIGINFO_BYTES;
const WAIT_COUNT_OFFSET: usize = SIGINFO_BYTES + 8;
const WAIT_SECONDS_OFFSET: usize = SIGINFO_BYTES + 16;
const WAIT_NANOSECONDS_OFFSET: usize = SIGINFO_BYTES + 24;
const WAIT_RESULT_OFFSET: usize = SIGINFO_BYTES + 32;
const WAIT_FRAME_BYTES: usize = SIGINFO_BYTES + 48;

/// Lowers `pcntl_sigprocmask()` and conditionally writes the prior mask array.
pub(super) fn lower_sigprocmask(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_sigprocmask", 2, 3)?;
    let old_slot = inst
        .operands
        .get(2)
        .copied()
        .map(|value| pcntl_signal_array_output_local_slot(ctx, value, "pcntl_sigprocmask"))
        .transpose()?;
    let failure = ctx.next_label("pcntl_sigprocmask_failure");
    let done = ctx.next_label("pcntl_sigprocmask_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, MASK_FRAME_BYTES);
    load_as_int(
        ctx,
        expect_operand(inst, 0)?,
        "pcntl_sigprocmask mode",
    )?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        MASK_MODE_OFFSET,
    );
    let signals = expect_operand(inst, 1)?;
    load_indexed_int_array(ctx, signals, "pcntl_sigprocmask signals")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x2, [x0]");                         // C arg2 = selected signal count
            ctx.emitter.instruction("add x1, x0, #24");                      // C arg1 = indexed signal payload
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", MASK_MODE_OFFSET);
            if old_slot.is_some() {
                abi::emit_temporary_stack_address(ctx.emitter, "x3", 0);     // C arg3 = prior-mask output buffer
                abi::emit_load_int_immediate(ctx.emitter, "x4", SIGNAL_CAPACITY as i64);
            } else {
                ctx.emitter.instruction("mov x3, #0");                       // caller omitted the output
                ctx.emitter.instruction("mov x4, #0");
            }
            ctx.emitter.bl_c("elephc_pcntl_sigprocmask");
            abi::emit_store_to_sp(ctx.emitter, "x0", MASK_RESULT_OFFSET);
            ctx.emitter.instruction("cmp x0, #0");
            ctx.emitter.instruction(&format!("b.lt {failure}"));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, QWORD PTR [rax]");              // C arg2 = selected signal count
            ctx.emitter.instruction("lea rsi, [rax + 24]");                   // C arg1 = indexed signal payload
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", MASK_MODE_OFFSET);
            if old_slot.is_some() {
                abi::emit_temporary_stack_address(ctx.emitter, "rcx", 0);     // C arg3 = prior-mask output buffer
                abi::emit_load_int_immediate(ctx.emitter, "r8", SIGNAL_CAPACITY as i64);
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                     // caller omitted the output
                ctx.emitter.instruction("xor r8d, r8d");
            }
            ctx.emitter.bl_c("elephc_pcntl_sigprocmask");
            abi::emit_store_to_sp(ctx.emitter, "rax", MASK_RESULT_OFFSET);
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("js {failure}"));
        }
    }
    if let Some(slot) = old_slot {
        ctx.release_local_before_refcounted_writeback(slot)?;
        emit_indexed_int_array_from_stack(ctx, 0, MASK_RESULT_OFFSET);
        store_signal_array(ctx, slot)?;
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {done}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {done}")),
    }
    ctx.emitter.label(&failure);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, MASK_FRAME_BYTES);
    store_if_result(ctx, inst)
}

/// Lowers Linux `pcntl_sigwaitinfo()` or `pcntl_sigtimedwait()` with siginfo writeback.
pub(super) fn lower_signal_wait(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    timed: bool,
) -> Result<()> {
    let name = if timed {
        "pcntl_sigtimedwait"
    } else {
        "pcntl_sigwaitinfo"
    };
    ensure_arg_count_between(inst, name, 1, if timed { 4 } else { 2 })?;
    let info_slot = inst
        .operands
        .get(1)
        .copied()
        .map(|value| pcntl_siginfo_output_local_slot(ctx, value, name))
        .transpose()?;
    let failure = ctx.next_label("pcntl_signal_wait_failure");
    let done = ctx.next_label("pcntl_signal_wait_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, WAIT_FRAME_BYTES);
    load_indexed_int_array(ctx, expect_operand(inst, 0)?, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x9, x0, #24");                       // preserve indexed signal payload
            ctx.emitter.instruction("ldr x10, [x0]");                        // preserve selected signal count
            abi::emit_store_to_sp(ctx.emitter, "x9", WAIT_SIGNALS_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "x10", WAIT_COUNT_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea r9, [rax + 24]");                    // preserve indexed signal payload
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");              // preserve selected signal count
            abi::emit_store_to_sp(ctx.emitter, "r9", WAIT_SIGNALS_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "r10", WAIT_COUNT_OFFSET);
        }
    }
    if timed {
        load_optional_int(ctx, inst.operands.get(2).copied(), 0, name)?;
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            WAIT_SECONDS_OFFSET,
        );
        load_optional_int(ctx, inst.operands.get(3).copied(), 0, name)?;
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            WAIT_NANOSECONDS_OFFSET,
        );
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WAIT_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", WAIT_COUNT_OFFSET);
            ctx.emitter.instruction("mov x2, sp");                            // C arg2 = stable siginfo record
            if timed {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", WAIT_SECONDS_OFFSET);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", WAIT_NANOSECONDS_OFFSET);
            }
            ctx.emitter.bl_c(if timed {
                "elephc_pcntl_sigtimedwait"
            } else {
                "elephc_pcntl_sigwaitinfo"
            });
            abi::emit_store_to_sp(ctx.emitter, "x0", WAIT_RESULT_OFFSET);
            ctx.emitter.instruction("cmp x0, #-1");
            ctx.emitter.instruction(&format!("b.eq {failure}"));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WAIT_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", WAIT_COUNT_OFFSET);
            ctx.emitter.instruction("mov rdx, rsp");                          // C arg2 = stable siginfo record
            if timed {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", WAIT_SECONDS_OFFSET);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "r8", WAIT_NANOSECONDS_OFFSET);
            }
            ctx.emitter.bl_c(if timed {
                "elephc_pcntl_sigtimedwait"
            } else {
                "elephc_pcntl_sigwaitinfo"
            });
            abi::emit_store_to_sp(ctx.emitter, "rax", WAIT_RESULT_OFFSET);
            ctx.emitter.instruction("cmp rax, -1");
            ctx.emitter.instruction(&format!("je {failure}"));
        }
    }
    if let Some(slot) = info_slot {
        ctx.release_local_before_refcounted_writeback(slot)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x0, sp"),
            Arch::X86_64 => ctx.emitter.instruction("mov rdi, rsp"),
        }
        abi::emit_call_label(ctx.emitter, "__rt_pcntl_siginfo_array");
        store_pcntl_siginfo_array(ctx, slot)?;
    }
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        WAIT_RESULT_OFFSET,
    );
    abi::emit_release_temporary_stack(ctx.emitter, WAIT_FRAME_BYTES);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {done}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {done}")),
    }
    ctx.emitter.label(&failure);
    abi::emit_release_temporary_stack(ctx.emitter, WAIT_FRAME_BYTES);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Validates and loads one indexed integer array through its native result pointer.
fn load_indexed_int_array(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    context: &str,
) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    if matches!(ty, PhpType::Array(ref element) if matches!(&**element, PhpType::Int | PhpType::Never))
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{context} storage {ty:?}",
    )))
}

/// Resolves an old-signal output to indexed integer or boxed local storage.
fn pcntl_signal_array_output_local_slot(
    ctx: &FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<LocalSlotId> {
    let slot = pcntl_output_local_slot(ctx, value, name, "old_signals")?;
    match ctx.local_php_type(slot)?.codegen_repr() {
        PhpType::Array(element)
            if matches!(&*element, PhpType::Int | PhpType::Never) =>
        {
            Ok(slot)
        }
        PhpType::Mixed => Ok(slot),
        other => Err(CodegenIrError::unsupported(format!(
            "{name} old_signals local with incompatible storage {other:?}",
        ))),
    }
}

/// Allocates and fills a fresh indexed integer array from one temporary stack buffer.
fn emit_indexed_int_array_from_stack(
    ctx: &mut FunctionContext<'_>,
    buffer_offset: usize,
    count_offset: usize,
) {
    let copy_loop = ctx.next_label("pcntl_signal_array_copy");
    let copy_done = ctx.next_label("pcntl_signal_array_copy_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", count_offset);
            ctx.emitter.instruction("mov x1, #8");                            // indexed integer slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", count_offset);
            abi::emit_temporary_stack_address(ctx.emitter, "x11", buffer_offset);
            ctx.emitter.instruction("add x12, x0, #24");                     // destination array payload
            ctx.emitter.instruction("mov x13, #0");                          // copy index
            ctx.emitter.label(&copy_loop);
            ctx.emitter.instruction("cmp x13, x10");
            ctx.emitter.instruction(&format!("b.ge {copy_done}"));
            ctx.emitter.instruction("ldr x14, [x11, x13, lsl #3]");
            ctx.emitter.instruction("str x14, [x12, x13, lsl #3]");
            ctx.emitter.instruction("add x13, x13, #1");
            ctx.emitter.instruction(&format!("b {copy_loop}"));
            ctx.emitter.label(&copy_done);
            ctx.emitter.instruction("str x10, [x0]");                        // publish logical length
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", count_offset);
            ctx.emitter.instruction("mov rsi, 8");                           // indexed integer slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", count_offset);
            abi::emit_temporary_stack_address(ctx.emitter, "r11", buffer_offset);
            ctx.emitter.instruction("lea r8, [rax + 24]");                    // destination array payload
            ctx.emitter.instruction("xor ecx, ecx");                         // copy index
            ctx.emitter.label(&copy_loop);
            ctx.emitter.instruction("cmp rcx, r10");
            ctx.emitter.instruction(&format!("jge {copy_done}"));
            ctx.emitter.instruction("mov r9, QWORD PTR [r11 + rcx * 8]");
            ctx.emitter.instruction("mov QWORD PTR [r8 + rcx * 8], r9");
            ctx.emitter.instruction("add rcx, 1");
            ctx.emitter.instruction(&format!("jmp {copy_loop}"));
            ctx.emitter.label(&copy_done);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");             // publish logical length
        }
    }
}

/// Stores a fresh indexed integer array into typed or boxed output storage.
fn store_signal_array(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Array(Box::new(PhpType::Int)),
        );
    }
    ctx.store_current_result_to_local(slot)
}

/// Loads an optional integer operand or a fixed PHP default.
fn load_optional_int(
    ctx: &mut FunctionContext<'_>,
    value: Option<crate::ir::ValueId>,
    default: i64,
    context: &str,
) -> Result<()> {
    match value {
        Some(value) => load_as_int(ctx, value, context),
        None => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), default);
            Ok(())
        }
    }
}
