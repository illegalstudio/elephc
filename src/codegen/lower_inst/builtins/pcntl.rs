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
use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    CodegenIrError, Result,
};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, PcntlRuntime, ValueDef, ValueId};
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
        PcntlRuntime::Wait => lower_wait(ctx, inst, false),
        PcntlRuntime::WaitPid => lower_wait(ctx, inst, true),
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

/// Lowers `pcntl_wait()` and `pcntl_waitpid()` with target-native status writeback.
fn lower_wait(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    selected_child: bool,
) -> Result<()> {
    let (name, status_index, flags_index, usage_index, min_args, max_args, symbol) = if selected_child {
        ("pcntl_waitpid", 1, 2, 3, 2, 4, "elephc_pcntl_waitpid")
    } else {
        ("pcntl_wait", 0, 1, 2, 1, 3, "elephc_pcntl_wait")
    };
    ensure_arg_count_between(inst, name, min_args, max_args)?;
    let status_value = expect_operand(inst, status_index)?;
    let status_slot = pcntl_int_output_local_slot(ctx, status_value, name)?;
    let usage_slot = inst
        .operands
        .get(usage_index)
        .copied()
        .map(|value| pcntl_rusage_output_local_slot(ctx, value, name))
        .transpose()?;
    let output_frame_size = if usage_slot.is_some() { 160 } else { 16 };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if selected_child {
                load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_waitpid process_id")?;
                ctx.emitter.instruction("str x0, [sp, #-16]!");                  // preserve the selected child id in an aligned slot
            }
            load_optional_int(ctx, inst.operands.get(flags_index).copied(), 0, name)?;
            ctx.emitter.instruction("mov x2, x0");                             // preserve flags while materializing the status pointer
            ctx.emitter.instruction(&format!("sub sp, sp, #{}", output_frame_size)); // reserve native status, result, and optional usage storage
            ctx.emitter.instruction("str wzr, [sp]");                         // initialize status for WNOHANG and failure paths
            if usage_slot.is_some() {
                ctx.emitter.instruction("add x3, sp, #16");                   // C arg3 = stable resource-usage output record
            }
            if selected_child {
                ctx.emitter.instruction(&format!("ldr x0, [sp, #{}]", output_frame_size)); // C arg0 = selected child id
                ctx.emitter.instruction("mov x1, sp");                         // C arg1 = writable native status
            } else {
                if usage_slot.is_some() {
                    ctx.emitter.instruction("mov x0, #-1");                   // C arg0 = any child for wait4
                    ctx.emitter.instruction("mov x1, sp");                    // C arg1 = writable native status
                } else {
                    ctx.emitter.instruction("mov x0, sp");                    // C arg0 = writable native status
                    ctx.emitter.instruction("mov x1, x2");                   // C arg1 = wait flags
                }
            }
            ctx.emitter.bl_c(if usage_slot.is_some() {
                "elephc_pcntl_wait4"
            } else {
                symbol
            });
            ctx.emitter.instruction("str x0, [sp, #8]");                      // preserve returned child id across status writeback
            ctx.emitter.instruction("ldrsw x0, [sp]");                        // load the target-native status as a PHP integer
            store_pcntl_status(ctx, status_slot)?;
            if let Some(slot) = usage_slot {
                ctx.release_local_before_refcounted_writeback(slot)?;
                ctx.emitter.instruction("add x0, sp, #16");                   // pass the stable usage record to the PHP-array builder
                abi::emit_call_label(ctx.emitter, "__rt_pcntl_rusage_array");
                store_pcntl_rusage_array(ctx, slot)?;
            }
            ctx.emitter.instruction("ldr x0, [sp, #8]");                      // restore returned child id
            ctx.emitter.instruction(&format!("add sp, sp, #{}", output_frame_size)); // release native output storage
            if selected_child {
                ctx.emitter.instruction("add sp, sp, #16");                   // release preserved child-id storage
            }
        }
        Arch::X86_64 => {
            if selected_child {
                load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_waitpid process_id")?;
                ctx.emitter.instruction("push rax");                           // preserve selected child id
                ctx.emitter.instruction("sub rsp, 8");                        // retain 16-byte stack alignment
            }
            load_optional_int(ctx, inst.operands.get(flags_index).copied(), 0, name)?;
            ctx.emitter.instruction("mov rdx, rax");                          // preserve flags while materializing the status pointer
            ctx.emitter.instruction(&format!("sub rsp, {}", output_frame_size)); // reserve native status, result, and optional usage storage
            ctx.emitter.instruction("mov DWORD PTR [rsp], 0");                // initialize status for WNOHANG and failure paths
            if usage_slot.is_some() {
                ctx.emitter.instruction("lea rcx, [rsp + 16]");                // C arg3 = stable resource-usage output record
            }
            if selected_child {
                ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", output_frame_size + 8)); // C arg0 = selected child id
                ctx.emitter.instruction("mov rsi, rsp");                       // C arg1 = writable native status
            } else {
                if usage_slot.is_some() {
                    ctx.emitter.instruction("mov rdi, -1");                   // C arg0 = any child for wait4
                    ctx.emitter.instruction("mov rsi, rsp");                  // C arg1 = writable native status
                } else {
                    ctx.emitter.instruction("mov rdi, rsp");                  // C arg0 = writable native status
                    ctx.emitter.instruction("mov esi, edx");                  // C arg1 = wait flags
                }
            }
            ctx.emitter.bl_c(if usage_slot.is_some() {
                "elephc_pcntl_wait4"
            } else {
                symbol
            });
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");          // preserve returned child id across status writeback
            ctx.emitter.instruction("movsxd rax, DWORD PTR [rsp]");            // load the target-native status as a PHP integer
            store_pcntl_status(ctx, status_slot)?;
            if let Some(slot) = usage_slot {
                ctx.release_local_before_refcounted_writeback(slot)?;
                ctx.emitter.instruction("lea rdi, [rsp + 16]");               // pass the stable usage record to the PHP-array builder
                abi::emit_call_label(ctx.emitter, "__rt_pcntl_rusage_array");
                store_pcntl_rusage_array(ctx, slot)?;
            }
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");          // restore returned child id
            ctx.emitter.instruction(&format!("add rsp, {}", output_frame_size)); // release native output storage
            if selected_child {
                ctx.emitter.instruction("add rsp, 16");                       // release preserved child-id storage and alignment pad
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Resolves a PCNTL integer output argument to a writable local or ref-cell slot.
fn pcntl_int_output_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<LocalSlotId> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{name} status argument that is not a local variable",
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if !matches!(inst_ref.op, Op::LoadLocal | Op::LoadRefCell) {
        return Err(CodegenIrError::unsupported(format!(
            "{name} status argument that is not a local variable",
        )));
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{name} status load missing local slot",
        )));
    };
    if !matches!(ctx.local_php_type(slot)?.codegen_repr(), PhpType::Int | PhpType::Mixed) {
        return Err(CodegenIrError::unsupported(format!(
            "{name} status local that is not integer storage",
        )));
    }
    Ok(slot)
}

/// Resolves a PCNTL resource-usage output to writable associative or boxed local storage.
fn pcntl_rusage_output_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<LocalSlotId> {
    let slot = pcntl_output_local_slot(ctx, value, name, "resource_usage")?;
    match ctx.local_php_type(slot)?.codegen_repr() {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == PhpType::Str && value.codegen_repr() == PhpType::Int =>
        {
            Ok(slot)
        }
        PhpType::Mixed => Ok(slot),
        other => Err(CodegenIrError::unsupported(format!(
            "{name} resource_usage local with incompatible storage {other:?}",
        ))),
    }
}

/// Resolves one write-only PCNTL operand to its source local slot.
fn pcntl_output_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    name: &str,
    parameter: &str,
) -> Result<LocalSlotId> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(format!(
            "{name} {parameter} argument that is not a local variable",
        )));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if !matches!(inst_ref.op, Op::LoadLocal | Op::LoadRefCell) {
        return Err(CodegenIrError::unsupported(format!(
            "{name} {parameter} argument that is not a local variable",
        )));
    }
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(format!(
            "{name} {parameter} load missing local slot",
        )));
    };
    Ok(slot)
}

/// Stores a fresh resource-usage hash into its typed or boxed PHP output local.
fn store_pcntl_rusage_array(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            },
        );
    }
    ctx.store_current_result_to_local(slot)
}

/// Stores a target-native wait status into raw integer or boxed Mixed output storage.
fn store_pcntl_status(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    }
    ctx.store_current_result_to_local(slot)
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
