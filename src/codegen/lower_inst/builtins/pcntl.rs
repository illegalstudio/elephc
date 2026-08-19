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

const PCNTL_CPU_CAPACITY: usize = 1024;
const PCNTL_CPU_BUFFER_BYTES: usize = PCNTL_CPU_CAPACITY * std::mem::size_of::<i64>();
const PCNTL_CPU_COUNT_OFFSET: usize = PCNTL_CPU_BUFFER_BYTES;
const PCNTL_CPU_PID_OFFSET: usize = PCNTL_CPU_BUFFER_BYTES + 8;
const PCNTL_CPU_FRAME_BYTES: usize = PCNTL_CPU_BUFFER_BYTES + 32;
const PCNTL_WARNING_BUFFER_BYTES: usize = 256;
pub(super) const PCNTL_WARNING_FORK: i64 = 0;
pub(super) const PCNTL_WARNING_EXEC: i64 = 1;
pub(super) const PCNTL_WARNING_SIGNAL: i64 = 2;

/// Dispatches one typed PCNTL operation without consulting its PHP source name.
pub(crate) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: PcntlRuntime,
) -> Result<()> {
    match target {
        PcntlRuntime::Alarm => lower_unary_int_bridge(ctx, inst, "pcntl_alarm", "elephc_pcntl_alarm", false),
        PcntlRuntime::AsyncSignals => super::pcntl_handlers::lower_async_signals(ctx, inst),
        PcntlRuntime::Exec => super::pcntl_exec::lower_exec(ctx, inst),
        PcntlRuntime::Fork => lower_fork(ctx, inst),
        PcntlRuntime::GetCpu => {
            lower_zero_arg_int_bridge(ctx, inst, "pcntl_getcpu", "elephc_pcntl_getcpu")
        }
        PcntlRuntime::GetCpuAffinity => lower_getcpuaffinity(ctx, inst),
        PcntlRuntime::GetLastError => lower_zero_arg_int_bridge(
            ctx,
            inst,
            "pcntl_get_last_error",
            "elephc_pcntl_get_last_error",
        ),
        PcntlRuntime::GetPriority => lower_getpriority(ctx, inst),
        PcntlRuntime::GetQosClass => lower_getqos_class(ctx, inst),
        PcntlRuntime::SetCpuAffinity => lower_setcpuaffinity(ctx, inst),
        PcntlRuntime::SetNs => lower_optional_binary_int_bridge(
            ctx,
            inst,
            "pcntl_setns",
            "elephc_pcntl_setns",
            0,
            0x4000_0000,
        ),
        PcntlRuntime::SetPriority => lower_setpriority(ctx, inst),
        PcntlRuntime::SetQosClass => lower_setqos_class(ctx, inst),
        PcntlRuntime::Signal => super::pcntl_handlers::lower_signal(ctx, inst),
        PcntlRuntime::SignalDispatch => super::pcntl_handlers::lower_signal_dispatch(ctx, inst),
        PcntlRuntime::SignalGetHandler => {
            super::pcntl_handlers::lower_signal_get_handler(ctx, inst)
        }
        PcntlRuntime::SignalMask => super::pcntl_signals::lower_sigprocmask(ctx, inst),
        PcntlRuntime::SignalTimedWait => super::pcntl_signals::lower_signal_wait(ctx, inst, true),
        PcntlRuntime::SignalWaitInfo => super::pcntl_signals::lower_signal_wait(ctx, inst, false),
        PcntlRuntime::StrError => lower_strerror(ctx, inst),
        PcntlRuntime::Unshare => lower_unary_int_bridge(
            ctx,
            inst,
            "pcntl_unshare",
            "elephc_pcntl_unshare",
            false,
        ),
        PcntlRuntime::Wait => lower_wait(ctx, inst, false),
        PcntlRuntime::WaitId => lower_waitid(ctx, inst),
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
    }
}

/// Lowers `pcntl_getqos_class()` into the corresponding lazy builtin enum singleton.
fn lower_getqos_class(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    const ENUM_NAME: &str = "Pcntl\\QosClass";
    const CASES: [(i64, &str); 5] = [
        (0, "UserInteractive"),
        (1, "UserInitiated"),
        (2, "Default"),
        (3, "Utility"),
        (4, "Background"),
    ];

    ensure_arg_count(inst, "pcntl_getqos_class", 0)?;
    ctx.emitter.bl_c("elephc_pcntl_getqos_class");
    let valid = ctx.next_label("pcntl_getqos_class_valid");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // reject the bridge's negative pthread error sentinel
            ctx.emitter.instruction(&format!("b.ge {valid}"));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // reject the bridge's negative pthread error sentinel
            ctx.emitter.instruction(&format!("jge {valid}"));
        }
    }
    super::super::exceptions::emit_error(ctx, "invalid QOS class");
    ctx.emitter.label(&valid);
    let done = ctx.next_label("pcntl_getqos_class_done");
    let case_labels = CASES
        .iter()
        .map(|(_, case)| ctx.next_label(&format!("pcntl_qos_{case}")))
        .collect::<Vec<_>>();

    for ((ordinal, _), label) in CASES.iter().zip(&case_labels) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x0, #{}", ordinal));      // select the PHP enum case for the bridge ordinal
                ctx.emitter.instruction(&format!("b.eq {label}"));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rax, {}", ordinal));      // select the PHP enum case for the bridge ordinal
                ctx.emitter.instruction(&format!("je {label}"));
            }
        }
    }
    ctx.emit_branch(&case_labels[2]);

    for ((_, case), label) in CASES.iter().zip(&case_labels) {
        ctx.emitter.label(label);
        crate::codegen::enum_singletons::emit_lazy_case_load(ctx, ENUM_NAME, case);
        abi::emit_incref_if_refcounted(
            ctx.emitter,
            &PhpType::Object(ENUM_NAME.to_string()),
        );
        ctx.emit_branch(&done);
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_setqos_class()` by passing the enum case's builtin `name` property to Darwin.
fn lower_setqos_class(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    const ENUM_NAME: &str = "Pcntl\\QosClass";

    ensure_arg_count_between(inst, "pcntl_setqos_class", 0, 1)?;
    if let Some(value) = inst.operands.first().copied() {
        let ty = ctx.load_value_to_result(value)?.codegen_repr();
        if !matches!(ty, PhpType::Object(ref name) if crate::names::php_symbol_key(name) == crate::names::php_symbol_key(ENUM_NAME)) {
            return Err(CodegenIrError::unsupported(format!(
                "pcntl_setqos_class enum storage {ty:?}",
            )));
        }
        let name_offset = ctx
            .module
            .class_infos
            .get(ENUM_NAME)
            .and_then(|info| info.property_offsets.get("name"))
            .copied()
            .ok_or_else(|| CodegenIrError::missing_entry("Pcntl\\QosClass name property", 0))?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("ldr x1, [x0, #{}]", name_offset + 8)); // C arg1 = enum case-name length
                ctx.emitter.instruction(&format!("ldr x0, [x0, #{}]", name_offset));     // C arg0 = enum case-name bytes
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rsi, QWORD PTR [rax + {}]", name_offset + 8)); // C arg1 = enum case-name length
                ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rax + {}]", name_offset));     // C arg0 = enum case-name bytes
            }
        }
    } else {
        let (default_label, default_len) = ctx.data.add_string(b"Default");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(ctx.emitter, "x0", &default_label);
                abi::emit_load_int_immediate(ctx.emitter, "x1", default_len as i64);
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(ctx.emitter, "rdi", &default_label);
                abi::emit_load_int_immediate(ctx.emitter, "rsi", default_len as i64);
            }
        }
    }
    ctx.emitter.bl_c("elephc_pcntl_setqos_class");
    let success = ctx.next_label("pcntl_setqos_class_success");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &success);
    super::super::exceptions::emit_error(ctx, "pcntl_setqos_class failed");
    ctx.emitter.label(&success);
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_getcpuaffinity()` into a boxed indexed integer array or boxed false.
fn lower_getcpuaffinity(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_getcpuaffinity", 0, 1)?;
    load_optional_int(
        ctx,
        inst.operands.first().copied(),
        0,
        "pcntl_getcpuaffinity process_id",
    )?;
    abi::emit_reserve_temporary_stack(ctx.emitter, PCNTL_CPU_FRAME_BYTES);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        PCNTL_CPU_PID_OFFSET,
    );
    let failure = ctx.next_label("pcntl_getcpuaffinity_failure");
    let copy_loop = ctx.next_label("pcntl_getcpuaffinity_copy");
    let copy_done = ctx.next_label("pcntl_getcpuaffinity_copy_done");
    let done = ctx.next_label("pcntl_getcpuaffinity_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", PCNTL_CPU_PID_OFFSET);
            abi::emit_temporary_stack_address(ctx.emitter, "x1", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x2", PCNTL_CPU_CAPACITY as i64);
            ctx.emitter.bl_c("elephc_pcntl_getcpuaffinity");
            ctx.emitter.instruction("cmp x0, #-1");
            ctx.emitter.instruction(&format!("b.eq {failure}"));
            abi::emit_store_to_sp(ctx.emitter, "x0", PCNTL_CPU_COUNT_OFFSET);
            ctx.emitter.instruction("mov x1, #8");                              // indexed integer slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", PCNTL_CPU_COUNT_OFFSET);
            abi::emit_temporary_stack_address(ctx.emitter, "x11", 0);
            ctx.emitter.instruction("add x12, x0, #24");                        // destination payload after the array header
            ctx.emitter.instruction("mov x13, #0");                             // copy index
            ctx.emitter.label(&copy_loop);
            ctx.emitter.instruction("cmp x13, x10");
            ctx.emitter.instruction(&format!("b.ge {copy_done}"));
            ctx.emitter.instruction("ldr x14, [x11, x13, lsl #3]");
            ctx.emitter.instruction("str x14, [x12, x13, lsl #3]");
            ctx.emitter.instruction("add x13, x13, #1");
            ctx.emitter.instruction(&format!("b {copy_loop}"));
            ctx.emitter.label(&copy_done);
            ctx.emitter.instruction("str x10, [x0]");                           // publish logical array length
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", PCNTL_CPU_PID_OFFSET);
            abi::emit_temporary_stack_address(ctx.emitter, "rsi", 0);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", PCNTL_CPU_CAPACITY as i64);
            ctx.emitter.bl_c("elephc_pcntl_getcpuaffinity");
            ctx.emitter.instruction("cmp rax, -1");
            ctx.emitter.instruction(&format!("je {failure}"));
            abi::emit_store_to_sp(ctx.emitter, "rax", PCNTL_CPU_COUNT_OFFSET);
            ctx.emitter.instruction("mov rdi, rax");                            // capacity equals returned CPU count
            ctx.emitter.instruction("mov rsi, 8");                              // indexed integer slots are eight bytes
            abi::emit_call_label(ctx.emitter, "__rt_array_new");
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", PCNTL_CPU_COUNT_OFFSET);
            ctx.emitter.instruction("mov r11, rsp");                            // source CPU-id buffer
            ctx.emitter.instruction("lea r8, [rax + 24]");                      // destination array payload
            ctx.emitter.instruction("xor ecx, ecx");                            // copy index
            ctx.emitter.label(&copy_loop);
            ctx.emitter.instruction("cmp rcx, r10");
            ctx.emitter.instruction(&format!("jge {copy_done}"));
            ctx.emitter.instruction("mov r9, QWORD PTR [r11 + rcx * 8]");
            ctx.emitter.instruction("mov QWORD PTR [r8 + rcx * 8], r9");
            ctx.emitter.instruction("add rcx, 1");
            ctx.emitter.instruction(&format!("jmp {copy_loop}"));
            ctx.emitter.label(&copy_done);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // publish logical array length
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, PCNTL_CPU_FRAME_BYTES);
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Array(Box::new(PhpType::Int)));
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {done}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {done}")),
    }
    ctx.emitter.label(&failure);
    abi::emit_release_temporary_stack(ctx.emitter, PCNTL_CPU_FRAME_BYTES);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_setcpuaffinity()` from an indexed integer array into the stable bridge ABI.
fn lower_setcpuaffinity(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_setcpuaffinity", 2, 2)?;
    load_optional_int(
        ctx,
        inst.operands.first().copied(),
        0,
        "pcntl_setcpuaffinity process_id",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let cpu_ids = expect_operand(inst, 1)?;
    let ty = ctx.load_value_to_result(cpu_ids)?.codegen_repr();
    if !matches!(ty, PhpType::Array(ref element) if matches!(element.codegen_repr(), PhpType::Int | PhpType::Never)) {
        return Err(CodegenIrError::unsupported(format!(
            "pcntl_setcpuaffinity CPU-id storage {ty:?}",
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x2, [x0]");                            // C arg2 = CPU-id count
            ctx.emitter.instruction("add x1, x0, #24");                         // C arg1 = indexed-array payload
            ctx.emitter.instruction("ldr x0, [sp]");                            // C arg0 = selected process id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, QWORD PTR [rax]");                // C arg2 = CPU-id count
            ctx.emitter.instruction("lea rsi, [rax + 24]");                     // C arg1 = indexed-array payload
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // C arg0 = selected process id
        }
    }
    ctx.emitter.bl_c("elephc_pcntl_setcpuaffinity");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    store_if_result(ctx, inst)
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
    let usage_from_result = ctx.next_label("pcntl_wait_usage_from_result");
    let usage_ready = ctx.next_label("pcntl_wait_usage_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if selected_child {
                load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_waitpid process_id")?;
                ctx.emitter.instruction("str x0, [sp, #-16]!");                 // preserve the selected child id in an aligned slot
            }
            load_optional_int(ctx, inst.operands.get(flags_index).copied(), 0, name)?;
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // preserve flags while loading the caller's prior status
            load_as_int(ctx, status_value, &format!("{name} status"))?;
            ctx.emitter.instruction(&format!("sub sp, sp, #{}", output_frame_size)); // reserve native status, result, and optional usage storage
            ctx.emitter.instruction("str w0, [sp]");                            // preserve PHP's prior status when wait leaves it untouched
            ctx.emitter.instruction(&format!("ldr x2, [sp, #{}]", output_frame_size)); // C flags argument saved above the output frame
            if usage_slot.is_some() {
                ctx.emitter.instruction("add x3, sp, #16");                     // C arg3 = stable resource-usage output record
            }
            if selected_child {
                ctx.emitter.instruction(&format!("ldr x0, [sp, #{}]", output_frame_size + 16)); // C arg0 = selected child id
                ctx.emitter.instruction("mov x1, sp");                          // C arg1 = writable native status
            } else {
                if usage_slot.is_some() {
                    ctx.emitter.instruction("mov x0, #-1");                     // C arg0 = any child for wait4
                    ctx.emitter.instruction("mov x1, sp");                      // C arg1 = writable native status
                } else {
                    ctx.emitter.instruction("mov x0, sp");                      // C arg0 = writable native status
                    ctx.emitter.instruction("mov x1, x2");                      // C arg1 = wait flags
                }
            }
            ctx.emitter.bl_c(if usage_slot.is_some() {
                "elephc_pcntl_wait4"
            } else {
                symbol
            });
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve returned child id across status writeback
            ctx.emitter.instruction("ldrsw x0, [sp]");                          // load the target-native status as a PHP integer
            store_pcntl_status(ctx, status_slot)?;
            if let Some(slot) = usage_slot {
                ctx.release_local_before_refcounted_writeback(slot)?;
                ctx.emitter.instruction("ldr x9, [sp, #8]");                    // inspect the returned child id before reading resource usage
                ctx.emitter.instruction("cmp x9, #0");                          // PHP fills usage only for a reaped child
                ctx.emitter.instruction(&format!("b.gt {usage_from_result}"));
                ctx.emitter.instruction("mov x0, #8");                          // allocate an empty associative array on failure/WNOHANG
                ctx.emitter.instruction("mov x1, #0");                          // hash value type = Int
                abi::emit_call_label(ctx.emitter, "__rt_hash_new");
                ctx.emitter.instruction(&format!("b {usage_ready}"));
                ctx.emitter.label(&usage_from_result);
                ctx.emitter.instruction("add x0, sp, #16");                     // pass the stable usage record to the PHP-array builder
                abi::emit_call_label(ctx.emitter, "__rt_pcntl_rusage_array");
                ctx.emitter.label(&usage_ready);
                store_pcntl_rusage_array(ctx, slot)?;
            }
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // restore returned child id
            ctx.emitter.instruction(&format!("add sp, sp, #{}", output_frame_size)); // release native output storage
            ctx.emitter.instruction("add sp, sp, #16");                         // release preserved flags storage
            if selected_child {
                ctx.emitter.instruction("add sp, sp, #16");                     // release preserved child-id storage
            }
        }
        Arch::X86_64 => {
            if selected_child {
                load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_waitpid process_id")?;
                ctx.emitter.instruction("push rax");                            // preserve selected child id
                ctx.emitter.instruction("sub rsp, 8");                          // retain 16-byte stack alignment
            }
            load_optional_int(ctx, inst.operands.get(flags_index).copied(), 0, name)?;
            ctx.emitter.instruction("sub rsp, 16");                             // reserve aligned flags storage
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve flags while loading the caller's prior status
            load_as_int(ctx, status_value, &format!("{name} status"))?;
            ctx.emitter.instruction(&format!("sub rsp, {}", output_frame_size)); // reserve native status, result, and optional usage storage
            ctx.emitter.instruction("mov DWORD PTR [rsp], eax");                // preserve PHP's prior status when wait leaves it untouched
            ctx.emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", output_frame_size)); // C flags argument saved above the output frame
            if usage_slot.is_some() {
                ctx.emitter.instruction("lea rcx, [rsp + 16]");                 // C arg3 = stable resource-usage output record
            }
            if selected_child {
                ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", output_frame_size + 24)); // C arg0 = selected child id
                ctx.emitter.instruction("mov rsi, rsp");                        // C arg1 = writable native status
            } else {
                if usage_slot.is_some() {
                    ctx.emitter.instruction("mov rdi, -1");                     // C arg0 = any child for wait4
                    ctx.emitter.instruction("mov rsi, rsp");                    // C arg1 = writable native status
                } else {
                    ctx.emitter.instruction("mov rdi, rsp");                    // C arg0 = writable native status
                    ctx.emitter.instruction("mov esi, edx");                    // C arg1 = wait flags
                }
            }
            ctx.emitter.bl_c(if usage_slot.is_some() {
                "elephc_pcntl_wait4"
            } else {
                symbol
            });
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve returned child id across status writeback
            ctx.emitter.instruction("movsxd rax, DWORD PTR [rsp]");             // load the target-native status as a PHP integer
            store_pcntl_status(ctx, status_slot)?;
            if let Some(slot) = usage_slot {
                ctx.release_local_before_refcounted_writeback(slot)?;
                ctx.emitter.instruction("cmp QWORD PTR [rsp + 8], 0");          // PHP fills usage only for a reaped child
                ctx.emitter.instruction(&format!("jg {usage_from_result}"));
                ctx.emitter.instruction("mov rdi, 8");                          // allocate an empty associative array on failure/WNOHANG
                ctx.emitter.instruction("xor esi, esi");                        // hash value type = Int
                abi::emit_call_label(ctx.emitter, "__rt_hash_new");
                ctx.emitter.instruction(&format!("jmp {usage_ready}"));
                ctx.emitter.label(&usage_from_result);
                ctx.emitter.instruction("lea rdi, [rsp + 16]");                 // pass the stable usage record to the PHP-array builder
                abi::emit_call_label(ctx.emitter, "__rt_pcntl_rusage_array");
                ctx.emitter.label(&usage_ready);
                store_pcntl_rusage_array(ctx, slot)?;
            }
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // restore returned child id
            ctx.emitter.instruction(&format!("add rsp, {}", output_frame_size)); // release native output storage
            ctx.emitter.instruction("add rsp, 16");                             // release preserved flags storage
            if selected_child {
                ctx.emitter.instruction("add rsp, 16");                         // release preserved child-id storage and alignment pad
            }
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_waitid()` through a stable bridge siginfo record and conditional writeback.
fn lower_waitid(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_waitid", 0, 4)?;
    let info_slot = inst
        .operands
        .get(2)
        .copied()
        .map(|value| pcntl_siginfo_output_local_slot(ctx, value, "pcntl_waitid"))
        .transpose()?;
    let no_writeback = ctx.next_label("pcntl_waitid_no_info_writeback");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #128");                        // reserve stable siginfo, scalar inputs, and result storage
            load_optional_int(ctx, inst.operands.first().copied(), 0, "pcntl_waitid idtype")?;
            ctx.emitter.instruction("str x0, [sp, #96]");                       // preserve idtype while materializing the remaining inputs
            load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_waitid id")?;
            ctx.emitter.instruction("str x0, [sp, #104]");                      // preserve selected id
            load_optional_int(ctx, inst.operands.get(3).copied(), 4, "pcntl_waitid flags")?;
            ctx.emitter.instruction("mov x3, x0");                              // C arg3 = wait flags
            ctx.emitter.instruction("ldr x0, [sp, #96]");                       // C arg0 = id type
            ctx.emitter.instruction("ldr x1, [sp, #104]");                      // C arg1 = selected id
            ctx.emitter.instruction("mov x2, sp");                              // C arg2 = stable siginfo output
            ctx.emitter.bl_c("elephc_pcntl_waitid");
            ctx.emitter.instruction("str x0, [sp, #112]");                      // preserve boolean result across optional array creation
            if let Some(slot) = info_slot {
                ctx.emitter.instruction(&format!("cbz x0, {no_writeback}"));    // leave caller output unchanged on failure
                ctx.release_local_before_refcounted_writeback(slot)?;
                ctx.emitter.instruction("mov x0, sp");                          // pass the stable siginfo record to the array builder
                abi::emit_call_label(ctx.emitter, "__rt_pcntl_siginfo_array");
                store_pcntl_siginfo_array(ctx, slot)?;
                ctx.emitter.label(&no_writeback);
            }
            ctx.emitter.instruction("ldr x0, [sp, #112]");                      // restore boolean success result
            ctx.emitter.instruction("add sp, sp, #128");                        // release stable output storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 128");                            // reserve stable siginfo, scalar inputs, and result storage
            load_optional_int(ctx, inst.operands.first().copied(), 0, "pcntl_waitid idtype")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 96], rax");           // preserve idtype while materializing the remaining inputs
            load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_waitid id")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 104], rax");          // preserve selected id
            load_optional_int(ctx, inst.operands.get(3).copied(), 4, "pcntl_waitid flags")?;
            ctx.emitter.instruction("mov ecx, eax");                            // C arg3 = wait flags
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");           // C arg0 = id type
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 104]");          // C arg1 = selected id
            ctx.emitter.instruction("mov rdx, rsp");                            // C arg2 = stable siginfo output
            ctx.emitter.bl_c("elephc_pcntl_waitid");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 112], rax");          // preserve boolean result across optional array creation
            if let Some(slot) = info_slot {
                ctx.emitter.instruction("test eax, eax");                       // leave caller output unchanged on failure
                ctx.emitter.instruction(&format!("jz {no_writeback}"));
                ctx.release_local_before_refcounted_writeback(slot)?;
                ctx.emitter.instruction("mov rdi, rsp");                        // pass the stable siginfo record to the array builder
                abi::emit_call_label(ctx.emitter, "__rt_pcntl_siginfo_array");
                store_pcntl_siginfo_array(ctx, slot)?;
                ctx.emitter.label(&no_writeback);
            }
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 112]");          // restore boolean success result
            ctx.emitter.instruction("add rsp, 128");                            // release stable output storage
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

/// Resolves a PCNTL signal-information output to writable associative or boxed storage.
pub(super) fn pcntl_siginfo_output_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    name: &str,
) -> Result<LocalSlotId> {
    let slot = pcntl_output_local_slot(ctx, value, name, "info")?;
    match ctx.local_php_type(slot)?.codegen_repr() {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == PhpType::Str
                && matches!(value.codegen_repr(), PhpType::Int | PhpType::Mixed) =>
        {
            Ok(slot)
        }
        PhpType::Mixed => Ok(slot),
        other => Err(CodegenIrError::unsupported(format!(
            "{name} info local with incompatible storage {other:?}",
        ))),
    }
}

/// Resolves one write-only PCNTL operand to its source local slot.
pub(super) fn pcntl_output_local_slot(
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

/// Stores a fresh signal-information hash into its typed or boxed PHP output local.
pub(super) fn store_pcntl_siginfo_array(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
) -> Result<()> {
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
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

/// Lowers `pcntl_fork()` and emits PHP's OS warning before returning `-1` on failure.
fn lower_fork(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "pcntl_fork", 0)?;
    let done = ctx.next_label("pcntl_fork_done");
    ctx.emitter.bl_c("elephc_pcntl_fork");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmn x0, #1");                              // compare the returned PID with the failure sentinel -1
            ctx.emitter.instruction(&format!("b.ne {done}"));                   // skip diagnostics after a successful parent or child return
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -1");                             // compare the returned PID with the failure sentinel -1
            ctx.emitter.instruction(&format!("jne {done}"));                    // skip diagnostics after a successful parent or child return
        }
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_pcntl_last_error_warning(ctx, PCNTL_WARNING_FORK);
    let result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_pop_reg(ctx.emitter, &result);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Formats and emits the bridge's latest PHP-compatible PCNTL warning.
pub(super) fn emit_pcntl_last_error_warning(ctx: &mut FunctionContext<'_>, kind: i64) {
    abi::emit_reserve_temporary_stack(ctx.emitter, PCNTL_WARNING_BUFFER_BYTES);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", kind);             // C arg0 = warning operation kind
            ctx.emitter.instruction("mov x1, sp");                              // C arg1 = caller-owned warning buffer
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x2",
                PCNTL_WARNING_BUFFER_BYTES as i64,
            );                                                                 // C arg2 = writable buffer capacity
            ctx.emitter.bl_c("elephc_pcntl_format_last_error_warning");
            ctx.emitter.instruction("mov x2, x0");                              // diagnostic arg1 = formatted byte count
            ctx.emitter.instruction("mov x1, sp");                              // diagnostic arg0 = formatted warning bytes
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", kind);            // C arg0 = warning operation kind
            ctx.emitter.instruction("mov rsi, rsp");                            // C arg1 = caller-owned warning buffer
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rdx",
                PCNTL_WARNING_BUFFER_BYTES as i64,
            );                                                                 // C arg2 = writable buffer capacity
            ctx.emitter.bl_c("elephc_pcntl_format_last_error_warning");
            ctx.emitter.instruction("mov rsi, rax");                            // diagnostic arg1 = formatted byte count
            ctx.emitter.instruction("mov rdi, rsp");                            // diagnostic arg0 = formatted warning bytes
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    abi::emit_release_temporary_stack(ctx.emitter, PCNTL_WARNING_BUFFER_BYTES);
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
        ctx.emitter.instruction("mov rdi, rax");                                // pass the integer operand through the SysV C ABI
    }
    ctx.emitter.bl_c(symbol);
    if box_result {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    }
    store_if_result(ctx, inst)
}

/// Lowers a bridge with two optional integer operands and fixed PHP defaults.
fn lower_optional_binary_int_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    symbol: &str,
    first_default: i64,
    second_default: i64,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 0, 2)?;
    load_optional_int(ctx, inst.operands.first().copied(), first_default, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_optional_int(ctx, inst.operands.get(1).copied(), second_default, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // C arg1 = second optional integer
            abi::emit_pop_reg(ctx.emitter, "x0");                             // C arg0 = first optional integer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov esi, eax");                            // C arg1 = second optional integer
            abi::emit_pop_reg(ctx.emitter, "rdi");                            // C arg0 = first optional integer
        }
    }
    ctx.emitter.bl_c(symbol);
    store_if_result(ctx, inst)
}

/// Lowers `pcntl_getpriority()` while preserving `-1` as a valid successful priority.
fn lower_getpriority(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_getpriority", 0, 2)?;
    let success = ctx.next_label("pcntl_getpriority_success");
    let done = ctx.next_label("pcntl_getpriority_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve aligned process-id and priority-output slots
            load_optional_int(ctx, inst.operands.first().copied(), 0, "pcntl_getpriority process_id")?;
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve process id while materializing mode
            load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_getpriority mode")?;
            ctx.emitter.instruction("mov x1, x0");                              // C arg1 = priority selector mode
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // C arg0 = process id
            ctx.emitter.instruction("add x2, sp, #8");                          // C arg2 = writable priority output
            ctx.emitter.bl_c("elephc_pcntl_getpriority");
            ctx.emitter.instruction(&format!("cbnz x0, {success}"));            // branch when the bridge distinguished a successful -1/value
            ctx.emitter.instruction("add sp, sp, #16");                         // release output storage before boxing false
            abi::emit_load_int_immediate(ctx.emitter, "x0", 0);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("b {done}"));                      // skip successful integer boxing
            ctx.emitter.label(&success);
            ctx.emitter.instruction("ldrsw x0, [sp, #8]");                      // sign-extend the returned C priority
            ctx.emitter.instruction("add sp, sp, #16");                         // release process-id and output slots
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve aligned process-id and priority-output slots
            load_optional_int(ctx, inst.operands.first().copied(), 0, "pcntl_getpriority process_id")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve process id while materializing mode
            load_optional_int(ctx, inst.operands.get(1).copied(), 0, "pcntl_getpriority mode")?;
            ctx.emitter.instruction("mov esi, eax");                            // C arg1 = priority selector mode
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // C arg0 = process id
            ctx.emitter.instruction("lea rdx, [rsp + 8]");                      // C arg2 = writable priority output
            ctx.emitter.bl_c("elephc_pcntl_getpriority");
            ctx.emitter.instruction("test eax, eax");                           // test the separate bridge success status
            ctx.emitter.instruction(&format!("jnz {success}"));                 // preserve valid negative priority values
            ctx.emitter.instruction("add rsp, 16");                             // release output storage before boxing false
            abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip successful integer boxing
            ctx.emitter.label(&success);
            ctx.emitter.instruction("movsxd rax, DWORD PTR [rsp + 8]");         // sign-extend the returned C priority
            ctx.emitter.instruction("add rsp, 16");                             // release process-id and output slots
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
            ctx.emitter.instruction("mov x2, x0");                              // C arg2 = priority selector mode
            abi::emit_pop_reg(ctx.emitter, "x1");                              // C arg1 = process id
            abi::emit_pop_reg(ctx.emitter, "x0");                              // C arg0 = requested priority
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edx, eax");                            // C arg2 = priority selector mode
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
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve aligned output storage for the message length
            load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_strerror error_code")?;
            ctx.emitter.instruction("mov x1, sp");                              // C arg1 writes the borrowed message length
            ctx.emitter.bl_c("elephc_pcntl_strerror");
            ctx.emitter.instruction("mov x1, x0");                              // place the borrowed pointer in PHP string result register x1
            ctx.emitter.instruction("ldr x2, [sp]");                            // load the borrowed message length into x2
            ctx.emitter.instruction("add sp, sp, #16");                         // release the temporary length slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve aligned output storage for the message length
            load_as_int(ctx, expect_operand(inst, 0)?, "pcntl_strerror error_code")?;
            ctx.emitter.instruction("mov edi, eax");                            // C arg0 = errno value
            ctx.emitter.instruction("mov rsi, rsp");                            // C arg1 writes the borrowed message length
            ctx.emitter.bl_c("elephc_pcntl_strerror");
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp]");                // load the borrowed message length into PHP string register rdx
            ctx.emitter.instruction("add rsp, 16");                             // release the temporary length slot
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
