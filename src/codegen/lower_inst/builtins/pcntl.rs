//! Purpose:
//! Lowers typed PCNTL EIR operations into target-aware bridge calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_calls` for `RuntimeCallTarget::Pcntl`.
//!
//! Key details:
//! - Unsupported operations fail explicitly until their bridge ABI is implemented.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, emit_box_current_value_as_mixed, CodegenIrError, Result};
use crate::ir::{Instruction, PcntlRuntime};
use crate::types::PhpType;

use super::strings::load_as_int;
use super::{ensure_arg_count, ensure_arg_count_between, expect_operand, store_if_result};

/// Dispatches one typed PCNTL operation without consulting its PHP source name.
pub(crate) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: PcntlRuntime,
) -> Result<()> {
    match target {
        PcntlRuntime::Alarm => lower_unary_int_bridge(ctx, inst, "pcntl_alarm", "elephc_pcntl_alarm", false),
        PcntlRuntime::Fork => lower_zero_arg_int_bridge(ctx, inst, "pcntl_fork", "elephc_pcntl_fork"),
        PcntlRuntime::GetLastError => lower_zero_arg_int_bridge(
            ctx,
            inst,
            "pcntl_get_last_error",
            "elephc_pcntl_get_last_error",
        ),
        PcntlRuntime::GetPriority => lower_getpriority(ctx, inst),
        PcntlRuntime::SetPriority => lower_setpriority(ctx, inst),
        PcntlRuntime::StrError => lower_strerror(ctx, inst),
        PcntlRuntime::WIfContinued => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wifcontinued",
            "elephc_pcntl_wifcontinued",
            false,
        ),
        PcntlRuntime::WIfExited => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wifexited",
            "elephc_pcntl_wifexited",
            false,
        ),
        PcntlRuntime::WIfSignaled => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wifsignaled",
            "elephc_pcntl_wifsignaled",
            false,
        ),
        PcntlRuntime::WIfStopped => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wifstopped",
            "elephc_pcntl_wifstopped",
            false,
        ),
        PcntlRuntime::WExitStatus => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wexitstatus",
            "elephc_pcntl_wexitstatus",
            true,
        ),
        PcntlRuntime::WStopSig => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wstopsig",
            "elephc_pcntl_wstopsig",
            true,
        ),
        PcntlRuntime::WTermSig => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_wtermsig",
            "elephc_pcntl_wtermsig",
            true,
        ),
        _ => Err(CodegenIrError::unsupported(format!(
            "typed PCNTL operation {}",
            target.as_eir(),
        ))),
    }
}

/// Lowers a zero-argument PCNTL bridge that returns one machine integer.
fn lower_zero_arg_int_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    symbol: &str,
) -> Result<()> {
    ensure_arg_count(inst, name, 0)?;
    ctx.emitter.bl_c(symbol);
    store_if_result(ctx, inst)
}

/// Lowers a one-integer PCNTL bridge and optionally boxes its integer into `Mixed` storage.
fn lower_unary_int_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    symbol: &str,
    box_result: bool,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    load_as_int(ctx, value, name)?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                               // pass the integer operand through the SysV C ABI
    }
    ctx.emitter.bl_c(symbol);
    if box_result {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    }
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_getpriority()` while preserving `-1` as a valid successful priority.
fn lower_getpriority(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_getpriority", 0, 2)?;
    let success = ctx.next_label("pcntl_getpriority_success");
    let done = ctx.next_label("pcntl_getpriority_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                        // reserve aligned process-id and priority-output slots
            load_optional_int(ctx, inst.operands.first().copied(), 0, "pcntl_getpriority process_id")?;
            ctx.emitter.instruction("str x0, [sp, #0]");                       // preserve process id while materializing mode
            load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_getpriority mode")?;
            ctx.emitter.instruction("mov x1, x0");                             // C arg1 = priority selector mode
            ctx.emitter.instruction("ldr x0, [sp, #0]");                      // C arg0 = process id
            ctx.emitter.instruction("add x2, sp, #8");                        // C arg2 = writable priority output
            ctx.emitter.bl_c("elephc_pcntl_getpriority");
            ctx.emitter.instruction(&format!("cbnz x0, {success}"));          // branch when the bridge distinguished a successful -1/value
            ctx.emitter.instruction("add sp, sp, #16");                       // release output storage before boxing false
            abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("b {done}"));                    // skip successful integer boxing
            ctx.emitter.label(&success);
            ctx.emitter.instruction("ldrsw x0, [sp, #8]");                    // sign-extend the returned C priority
            ctx.emitter.instruction("add sp, sp, #16");                       // release process-id and output slots
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                            // reserve aligned process-id and priority-output slots
            load_optional_int(ctx, inst.operands.first().copied(), 0, "pcntl_getpriority process_id")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");               // preserve process id while materializing mode
            load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_getpriority mode")?;
            ctx.emitter.instruction("mov esi, eax");                           // C arg1 = priority selector mode
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");               // C arg0 = process id
            ctx.emitter.instruction("lea rdx, [rsp + 8]");                     // C arg2 = writable priority output
            ctx.emitter.bl_c("elephc_pcntl_getpriority");
            ctx.emitter.instruction("test eax, eax");                          // test the separate bridge success status
            ctx.emitter.instruction(&format!("jnz {success}"));               // preserve valid negative priority values
            ctx.emitter.instruction("add rsp, 16");                            // release output storage before boxing false
            abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("jmp {done}"));                  // skip successful integer boxing
            ctx.emitter.label(&success);
            ctx.emitter.instruction("movsxd rax, DWORD PTR [rsp + 8]");         // sign-extend the returned C priority
            ctx.emitter.instruction("add rsp, 16");                            // release process-id and output slots
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_setpriority()` with PHP's optional process-id and mode defaults.
fn lower_setpriority(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_setpriority", 1, 3)?;
    load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_setpriority priority")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_setpriority process_id")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_optional_int(ctx, inst.operands.get(2).copied(), 0, "pcntl_setpriority mode")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x0");                             // C arg2 = priority selector mode
            abi::emit_pop_reg(ctx.emitter, "x1");                              // C arg1 = process id
            abi::emit_pop_reg(ctx.emitter, "x0");                              // C arg0 = requested priority
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edx, eax");                           // C arg2 = priority selector mode
            abi::emit_pop_reg(ctx.emitter, "rsi");                             // C arg1 = process id
            abi::emit_pop_reg(ctx.emitter, "rdi");                             // C arg0 = requested priority
        }
    }
    ctx.emitter.bl_c("elephc_pcntl_setpriority");
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_strerror()` and persists libc's borrowed message as an owned PHP string.
fn lower_strerror(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "pcntl_strerror", 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                        // reserve aligned output storage for the message length
            load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_strerror error_code")?;
            ctx.emitter.instruction("mov x1, sp");                             // C arg1 writes the borrowed message length
            ctx.emitter.bl_c("elephc_pcntl_strerror");
            ctx.emitter.instruction("mov x1, x0");                             // place the borrowed pointer in PHP string result register x1
            ctx.emitter.instruction("ldr x2, [sp]");                          // load the borrowed message length into x2
            ctx.emitter.instruction("add sp, sp, #16");                       // release the temporary length slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                            // reserve aligned output storage for the message length
            load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_strerror error_code")?;
            ctx.emitter.instruction("mov edi, eax");                           // C arg0 = errno value
            ctx.emitter.instruction("mov rsi, rsp");                           // C arg1 writes the borrowed message length
            ctx.emitter.bl_c("elephc_pcntl_strerror");
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp]");               // load the borrowed message length into PHP string register rdx
            ctx.emitter.instruction("add rsp, 16");                            // release the temporary length slot
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                     // copy libc-owned bytes into fresh PHP string storage
    store_if_result(ctx, inst)
}

/// Loads an optional integer operand or its immediate default.
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
