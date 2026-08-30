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
use crate::codegen::platform::{Arch, Platform};
use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed, CodegenIrError,
    Result,
};
use crate::ir::{Instruction, LocalSlotId};
use crate::types::PhpType;

use super::pcntl::{
    pcntl_optional_output_local_slot, pcntl_siginfo_output_local_slot, store_pcntl_siginfo_array,
};
use super::strings::load_as_int;
use super::{ensure_arg_count_between, expect_operand, store_if_result};

const SIGNAL_CAPACITY: usize = 128;
const SIGNAL_BUFFER_BYTES: usize = SIGNAL_CAPACITY * std::mem::size_of::<i64>();
const MASK_MODE_OFFSET: usize = SIGNAL_BUFFER_BYTES;
const MASK_RESULT_OFFSET: usize = SIGNAL_BUFFER_BYTES + 8;
const MASK_SIGNALS_OFFSET: usize = SIGNAL_BUFFER_BYTES + 16;
const MASK_COUNT_OFFSET: usize = SIGNAL_BUFFER_BYTES + 24;
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
    let old_slot = match inst.operands.get(2).copied() {
        Some(value) => pcntl_signal_array_output_local_slot(ctx, value, "pcntl_sigprocmask")?,
        None => None,
    };
    let failure = ctx.next_label("pcntl_sigprocmask_failure");
    let done = ctx.next_label("pcntl_sigprocmask_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, MASK_FRAME_BYTES);
    load_as_int(
        ctx,
        expect_operand(inst, 0)?,
        "pcntl_sigprocmask mode",
    )?;
    emit_validate_sigprocmask_mode(ctx);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        MASK_MODE_OFFSET,
    );
    let signals = expect_operand(inst, 1)?;
    load_indexed_int_array(ctx, signals, "pcntl_sigprocmask signals")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // stage the selected signal count
            ctx.emitter.instruction("add x10, x0, #24");                        // stage the indexed signal payload
            abi::emit_store_to_sp(ctx.emitter, "x9", MASK_COUNT_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "x10", MASK_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", MASK_MODE_OFFSET);
            let setmask = match ctx.emitter.target.platform {
                Platform::MacOS => 3,
                Platform::Linux => 2,
                Platform::Windows => 0,
            };
            ctx.emitter.instruction(&format!("cmp x9, #{setmask}"));            // only SIG_SETMASK accepts an empty signal array
            ctx.emitter.instruction("cset x2, eq");                             // validation arg2 = whether emptiness is allowed
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", MASK_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", MASK_COUNT_OFFSET);
            emit_validate_signal_set(ctx, "pcntl_sigprocmask", 2);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", MASK_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", MASK_COUNT_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", MASK_MODE_OFFSET);
            if old_slot.is_some() {
                abi::emit_temporary_stack_address(ctx.emitter, "x3", 0);        // C arg3 = prior-mask output buffer
                abi::emit_load_int_immediate(ctx.emitter, "x4", SIGNAL_CAPACITY as i64);
            } else {
                ctx.emitter.instruction("mov x3, #0");                          // caller omitted the output
                ctx.emitter.instruction("mov x4, #0");                          // report no prior-mask output capacity
            }
            ctx.emitter.bl_c("elephc_pcntl_sigprocmask");
            abi::emit_store_to_sp(ctx.emitter, "x0", MASK_RESULT_OFFSET);
            ctx.emitter.instruction("cmp x0, #0");                              // detect bridge failure before writing the old mask
            ctx.emitter.instruction(&format!("b.lt {failure}"));                // return false while preserving caller output
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // stage the selected signal count
            ctx.emitter.instruction("lea r10, [rax + 24]");                     // stage the indexed signal payload
            abi::emit_store_to_sp(ctx.emitter, "r9", MASK_COUNT_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "r10", MASK_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", MASK_MODE_OFFSET);
            ctx.emitter.instruction("cmp r9, 2");                               // Linux SIG_SETMASK alone accepts an empty array
            ctx.emitter.instruction("sete dl");                                 // materialize the validation allow-empty flag
            ctx.emitter.instruction("movzx edx, dl");                           // validation arg2 = normalized boolean
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", MASK_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", MASK_COUNT_OFFSET);
            emit_validate_signal_set(ctx, "pcntl_sigprocmask", 2);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", MASK_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", MASK_COUNT_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", MASK_MODE_OFFSET);
            if old_slot.is_some() {
                abi::emit_temporary_stack_address(ctx.emitter, "rcx", 0);       // C arg3 = prior-mask output buffer
                abi::emit_load_int_immediate(ctx.emitter, "r8", SIGNAL_CAPACITY as i64);
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                        // caller omitted the output
                ctx.emitter.instruction("xor r8d, r8d");                        // report no prior-mask output capacity
            }
            ctx.emitter.bl_c("elephc_pcntl_sigprocmask");
            abi::emit_store_to_sp(ctx.emitter, "rax", MASK_RESULT_OFFSET);
            ctx.emitter.instruction("test rax, rax");                           // detect bridge failure before writing the old mask
            ctx.emitter.instruction(&format!("js {failure}"));                  // return false while preserving caller output
        }
    }
    if let Some(slot) = old_slot {
        ctx.release_local_before_refcounted_writeback(slot)?;
        emit_indexed_int_array_from_stack(ctx, 0, MASK_RESULT_OFFSET);
        store_signal_array(ctx, slot)?;
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {done}")),         // bypass the false failure result
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {done}")),        // bypass the false failure result
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
    let info_slot = match inst.operands.get(1).copied() {
        Some(value) => pcntl_siginfo_output_local_slot(ctx, value, name)?,
        None => None,
    };
    let failure = ctx.next_label("pcntl_signal_wait_failure");
    let done = ctx.next_label("pcntl_signal_wait_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, WAIT_FRAME_BYTES);
    load_indexed_int_array(ctx, expect_operand(inst, 0)?, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x9, x0, #24");                         // preserve indexed signal payload
            ctx.emitter.instruction("ldr x10, [x0]");                           // preserve selected signal count
            abi::emit_store_to_sp(ctx.emitter, "x9", WAIT_SIGNALS_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "x10", WAIT_COUNT_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea r9, [rax + 24]");                      // preserve indexed signal payload
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // preserve selected signal count
            abi::emit_store_to_sp(ctx.emitter, "r9", WAIT_SIGNALS_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, "r10", WAIT_COUNT_OFFSET);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WAIT_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", WAIT_COUNT_OFFSET);
            ctx.emitter.instruction("mov x2, #0");                              // synchronous waits reject an empty signal set
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WAIT_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", WAIT_COUNT_OFFSET);
            ctx.emitter.instruction("xor edx, edx");                            // synchronous waits reject an empty signal set
        }
    }
    emit_validate_signal_set(ctx, name, 1);
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
        emit_validate_sigtimedwait_timeout(ctx);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", WAIT_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", WAIT_COUNT_OFFSET);
            ctx.emitter.instruction("mov x2, sp");                              // C arg2 = stable siginfo record
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
            ctx.emitter.instruction("cmp x0, #-1");                             // detect a failed or timed-out signal wait
            ctx.emitter.instruction(&format!("b.eq {failure}"));                // return false without siginfo writeback
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", WAIT_SIGNALS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", WAIT_COUNT_OFFSET);
            ctx.emitter.instruction("mov rdx, rsp");                            // C arg2 = stable siginfo record
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
            ctx.emitter.instruction("cmp rax, -1");                             // detect a failed or timed-out signal wait
            ctx.emitter.instruction(&format!("je {failure}"));                  // return false without siginfo writeback
        }
    }
    if let Some(slot) = info_slot {
        ctx.release_local_before_refcounted_writeback(slot)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction("mov x0, sp"),             // pass the stable siginfo record to the array builder
            Arch::X86_64 => ctx.emitter.instruction("mov rdi, rsp"),            // pass the stable siginfo record to the array builder
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
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {done}")),         // bypass the boxed false failure result
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {done}")),        // bypass the boxed false failure result
    }
    ctx.emitter.label(&failure);
    abi::emit_release_temporary_stack(ctx.emitter, WAIT_FRAME_BYTES);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Rejects a signal-mask mode outside the target's three PHP constants.
fn emit_validate_sigprocmask_mode(ctx: &mut FunctionContext<'_>) {
    let valid = ctx.next_label("pcntl_sigprocmask_valid_mode");
    let (minimum, maximum) = match ctx.emitter.target.platform {
        Platform::MacOS => (1, 3),
        Platform::Linux => (0, 2),
        Platform::Windows => (0, 0),
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{minimum}"));            // compare against the target's first valid mask mode
            ctx.emitter.instruction(&format!("b.lt {valid}_error"));            // reject values below SIG_BLOCK/SIG_SETMASK range
            ctx.emitter.instruction(&format!("cmp x0, #{maximum}"));            // compare against the target's final valid mask mode
            ctx.emitter.instruction(&format!("b.le {valid}"));                  // accept SIG_BLOCK, SIG_UNBLOCK, or SIG_SETMASK
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {minimum}"));            // compare against the target's first valid mask mode
            ctx.emitter.instruction(&format!("jl {valid}_error"));              // reject values below SIG_BLOCK/SIG_SETMASK range
            ctx.emitter.instruction(&format!("cmp rax, {maximum}"));            // compare against the target's final valid mask mode
            ctx.emitter.instruction(&format!("jle {valid}"));                   // accept SIG_BLOCK, SIG_UNBLOCK, or SIG_SETMASK
        }
    }
    ctx.emitter.label(&format!("{valid}_error"));
    super::super::exceptions::emit_value_error(
        ctx,
        "pcntl_sigprocmask(): Argument #1 ($mode) must be one of SIG_BLOCK, SIG_UNBLOCK, or SIG_SETMASK",
    );
    ctx.emitter.label(&valid);
}

/// Calls the bridge's signal-set validator and raises PHP's array `ValueError`s.
fn emit_validate_signal_set(ctx: &mut FunctionContext<'_>, name: &str, argument: usize) {
    let empty = ctx.next_label("pcntl_signal_set_empty");
    let range = ctx.next_label("pcntl_signal_set_range");
    let valid = ctx.next_label("pcntl_signal_set_valid");
    ctx.emitter.bl_c("elephc_pcntl_validate_signal_set");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sxtw x0, w0");                             // preserve negative C int validation results in x0
            ctx.emitter.instruction("cmp x0, #-1");                             // distinguish a forbidden empty signal array
            ctx.emitter.instruction(&format!("b.eq {empty}"));                  // raise PHP's non-empty argument ValueError
            ctx.emitter.instruction("cmp x0, #-2");                             // distinguish an out-of-range signal member
            ctx.emitter.instruction(&format!("b.eq {range}"));                  // raise PHP's signal-range ValueError
            ctx.emitter.instruction(&format!("b {valid}"));                     // continue after successful validation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp eax, -1");                             // distinguish a forbidden empty signal array
            ctx.emitter.instruction(&format!("je {empty}"));                    // raise PHP's non-empty argument ValueError
            ctx.emitter.instruction("cmp eax, -2");                             // distinguish an out-of-range signal member
            ctx.emitter.instruction(&format!("je {range}"));                    // raise PHP's signal-range ValueError
            ctx.emitter.instruction(&format!("jmp {valid}"));                   // continue after successful validation
        }
    }
    ctx.emitter.label(&empty);
    super::super::exceptions::emit_value_error(
        ctx,
        &format!("{name}(): Argument #{argument} ($signals) must not be empty"),
    );
    ctx.emitter.label(&range);
    let maximum = match ctx.emitter.target.platform {
        Platform::MacOS => 31,
        Platform::Linux => 64,
        Platform::Windows => 0,
    };
    super::super::exceptions::emit_value_error(
        ctx,
        &format!(
            "{name}(): Argument #{argument} ($signals) signals must be between 1 and {maximum}"
        ),
    );
    ctx.emitter.label(&valid);
}

/// Raises PHP's runtime `ValueError`s for invalid dynamic timed-wait bounds.
fn emit_validate_sigtimedwait_timeout(ctx: &mut FunctionContext<'_>) {
    let invalid_seconds = ctx.next_label("pcntl_sigtimedwait_invalid_seconds");
    let invalid_nanoseconds = ctx.next_label("pcntl_sigtimedwait_invalid_nanoseconds");
    let zero_timeout = ctx.next_label("pcntl_sigtimedwait_zero_timeout");
    let valid = ctx.next_label("pcntl_sigtimedwait_valid_timeout");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", WAIT_SECONDS_OFFSET);
            ctx.emitter.instruction("cmp x9, #0");                              // reject negative seconds supplied through dynamic storage
            ctx.emitter.instruction(&format!("b.lt {invalid_seconds}"));        // raise PHP's argument-three ValueError
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", WAIT_NANOSECONDS_OFFSET);
            ctx.emitter.instruction("cmp x10, #0");                             // reject negative nanoseconds
            ctx.emitter.instruction(&format!("b.lt {invalid_nanoseconds}"));    // raise PHP's argument-four ValueError
            abi::emit_load_int_immediate(ctx.emitter, "x11", 1_000_000_000);
            ctx.emitter.instruction("cmp x10, x11");                            // enforce the exclusive one-billion nanosecond bound
            ctx.emitter.instruction(&format!("b.ge {invalid_nanoseconds}"));    // reject a non-normalized timespec
            ctx.emitter.instruction("orr x9, x9, x10");                         // test whether both timeout components are zero
            ctx.emitter.instruction(&format!("cbz x9, {zero_timeout}"));        // PHP forbids an entirely zero timeout
            ctx.emitter.instruction(&format!("b {valid}"));                     // continue to the bridge with validated values
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", WAIT_SECONDS_OFFSET);
            ctx.emitter.instruction("cmp r9, 0");                               // reject negative seconds supplied through dynamic storage
            ctx.emitter.instruction(&format!("jl {invalid_seconds}"));          // raise PHP's argument-three ValueError
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", WAIT_NANOSECONDS_OFFSET);
            ctx.emitter.instruction("cmp r10, 0");                              // reject negative nanoseconds
            ctx.emitter.instruction(&format!("jl {invalid_nanoseconds}"));      // raise PHP's argument-four ValueError
            ctx.emitter.instruction("cmp r10, 1000000000");                     // enforce the exclusive one-billion nanosecond bound
            ctx.emitter.instruction(&format!("jge {invalid_nanoseconds}"));     // reject a non-normalized timespec
            ctx.emitter.instruction("or r9, r10");                              // test whether both timeout components are zero
            ctx.emitter.instruction(&format!("jz {zero_timeout}"));             // PHP forbids an entirely zero timeout
            ctx.emitter.instruction(&format!("jmp {valid}"));                   // continue to the bridge with validated values
        }
    }
    ctx.emitter.label(&invalid_seconds);
    super::super::exceptions::emit_value_error(
        ctx,
        "pcntl_sigtimedwait(): Argument #3 ($seconds) must be greater than or equal to 0",
    );
    ctx.emitter.label(&invalid_nanoseconds);
    super::super::exceptions::emit_value_error(
        ctx,
        "pcntl_sigtimedwait(): Argument #4 ($nanoseconds) must be between 0 and 1e9",
    );
    ctx.emitter.label(&zero_timeout);
    super::super::exceptions::emit_value_error(
        ctx,
        "pcntl_sigtimedwait(): At least one of argument #3 ($seconds) or argument #4 ($nanoseconds) must be greater than 0",
    );
    ctx.emitter.label(&valid);
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
) -> Result<Option<LocalSlotId>> {
    let Some(slot) = pcntl_optional_output_local_slot(ctx, value)? else {
        return Ok(None);
    };
    match ctx.local_php_type(slot)?.codegen_repr() {
        PhpType::Array(element)
            if matches!(&*element, PhpType::Int | PhpType::Never) =>
        {
            Ok(Some(slot))
        }
        PhpType::Mixed => Ok(Some(slot)),
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
            ctx.emitter.instruction("mov x1, #8");                              // indexed integer slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", count_offset);
            abi::emit_temporary_stack_address(ctx.emitter, "x11", buffer_offset);
            ctx.emitter.instruction("add x12, x0, #24");                        // destination array payload
            ctx.emitter.instruction("mov x13, #0");                             // copy index
            ctx.emitter.label(&copy_loop);
            ctx.emitter.instruction("cmp x13, x10");                            // test whether every signal was copied
            ctx.emitter.instruction(&format!("b.ge {copy_done}"));              // finish at the bridge-reported count
            ctx.emitter.instruction("ldr x14, [x11, x13, lsl #3]");             // load one stable signal number
            ctx.emitter.instruction("str x14, [x12, x13, lsl #3]");             // append the signal to the PHP array payload
            ctx.emitter.instruction("add x13, x13, #1");                        // advance the copy index
            ctx.emitter.instruction(&format!("b {copy_loop}"));                 // copy the next signal number
            ctx.emitter.label(&copy_done);
            ctx.emitter.instruction("str x10, [x0]");                           // publish logical length
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", count_offset);
            ctx.emitter.instruction("mov rsi, 8");                              // indexed integer slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", count_offset);
            abi::emit_temporary_stack_address(ctx.emitter, "r11", buffer_offset);
            ctx.emitter.instruction("lea r8, [rax + 24]");                      // destination array payload
            ctx.emitter.instruction("xor ecx, ecx");                            // copy index
            ctx.emitter.label(&copy_loop);
            ctx.emitter.instruction("cmp rcx, r10");                            // test whether every signal was copied
            ctx.emitter.instruction(&format!("jge {copy_done}"));               // finish at the bridge-reported count
            ctx.emitter.instruction("mov r9, QWORD PTR [r11 + rcx * 8]");       // load one stable signal number
            ctx.emitter.instruction("mov QWORD PTR [r8 + rcx * 8], r9");        // append the signal to the PHP array payload
            ctx.emitter.instruction("add rcx, 1");                              // advance the copy index
            ctx.emitter.instruction(&format!("jmp {copy_loop}"));               // copy the next signal number
            ctx.emitter.label(&copy_done);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // publish logical length
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
