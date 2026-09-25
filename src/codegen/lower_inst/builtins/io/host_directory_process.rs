//! Purpose:
//! Disk, host, directory, process, and basic file path calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;
use crate::codegen::platform::Platform;
use crate::ir::{ResourceCleanupKind, RuntimeFnId};

/// The cleanup kind `opendir()` stamps into its boxed resource.
///
/// Read from `RuntimeFnId::resource_cleanup_kind` instead of written here, because the
/// runtime emitter derives the presence of the matching `__rt_mixed_free_deep` arm from
/// that same answer. Resolving it in a `const` makes the two agree at compile time: if the
/// declaration is ever dropped, this fails to build rather than shipping a handle whose
/// destructor is not in the binary.
const OPENDIR_CLEANUP_KIND: ResourceCleanupKind = match RuntimeFnId::Opendir
    .resource_cleanup_kind()
{
    Some(kind) => kind,
    None => panic!("RuntimeFnId::Opendir boxes a kinded resource and must declare its kind"),
};

/// The cleanup kind `popen()` stamps into its boxed resource, resolved like the one above.
const POPEN_CLEANUP_KIND: ResourceCleanupKind = match RuntimeFnId::Popen.resource_cleanup_kind() {
    Some(kind) => kind,
    None => panic!("RuntimeFnId::Popen boxes a kinded resource and must declare its kind"),
};

/// The cleanup kind `proc_open()` stamps into the returned process resource.
///
/// The typed runtime target owns this classification so resource cleanup and
/// the emitted `__rt_mixed_free_deep` dispatch remain synchronized.
const PROC_OPEN_CLEANUP_KIND: ResourceCleanupKind = match RuntimeFnId::ProcOpen
    .resource_cleanup_kind()
{
    Some(kind) => kind,
    None => panic!("RuntimeFnId::ProcOpen boxes a kinded resource and must declare its kind"),
};

/// Lowers `proc_open()` into its eight-word runtime ABI.
///
/// The semantic descriptor always supplies six public PHP values and three hidden
/// Windows-marshalling operands. The native ABI consumes descriptor, command
/// pointer/length, pipes, cwd pointer/length, environment pointer, and packed flags.
pub(crate) fn lower_proc_open(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 9 {
        ensure_arg_count_between(inst, "proc_open", 3, 6)?;
    }
    let public_command = expect_operand(inst, 0)?;
    let command_override = inst.operands.get(6).copied();
    let command = match command_override {
        Some(value) if ctx.value_php_type(value)? == PhpType::Str => value,
        _ => public_command,
    };
    let descriptor_spec = expect_operand(inst, 1)?;
    let pipes = expect_operand(inst, 2)?;
    let pipes_local = source_load_local_slot(ctx, pipes)?;
    let cwd = inst.operands.get(3).copied();
    let public_env_vars = inst.operands.get(4).copied();
    let public_options = inst.operands.get(5).copied();
    let env_vars = if inst.operands.len() == 9 {
        inst.operands.get(7).copied()
    } else {
        public_env_vars
    };
    let packed_flags = if inst.operands.len() == 9 {
        inst.operands.get(8).copied()
    } else {
        public_options
    };

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_result(descriptor_spec)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, command, "proc_open command")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, #0");                              // proc_open replaces the by-ref pipes value with a fresh container
            abi::emit_push_reg(ctx.emitter, "x0");
            load_optional_proc_open_string(ctx, cwd, "proc_open cwd")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_optional_proc_open_pointer(ctx, env_vars)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_optional_proc_open_pointer(ctx, packed_flags)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_pop_reg(ctx.emitter, "x7");
            abi::emit_pop_reg(ctx.emitter, "x6");
            abi::emit_pop_reg_pair(ctx.emitter, "x4", "x5");
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 48");                             // reserve dynamic ownership, result, and validation slots
            ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                  // no owned command buffer unless runtime argv marshalling runs
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // no owned environment block unless runtime hash marshalling runs
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], 0");             // dynamic command marshalling has not failed
            ctx.load_value_to_result(descriptor_spec)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            if matches!(ctx.value_php_type(command)?, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                ctx.load_value_to_result(command)?;
                ctx.emitter.instruction("mov rdi, rax");                        // pass the runtime argv array to the Windows marshaller
                abi::emit_call_label(ctx.emitter, "__rt_win_proc_command_array");
                ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");       // retain the allocation above the staged descriptor slot
                ctx.emitter.instruction("test rax, rax");                       // did argv marshalling succeed?
                let command_valid = ctx.next_label("proc_open_command_valid");
                ctx.emitter.instruction(&format!("jnz {command_valid}"));       // non-null buffer is a valid command line
                ctx.emitter.instruction("mov QWORD PTR [rsp + 48], 1");         // record command marshalling failure
                ctx.emitter.label(&command_valid);
            } else {
                load_string_to_result(ctx, command, "proc_open command")?;
            }
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("xor eax, eax");                            // proc_open replaces the by-ref pipes value with a fresh container
            abi::emit_push_reg(ctx.emitter, "rax");
            load_optional_proc_open_string(ctx, cwd, "proc_open cwd")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            let dynamic_environment = public_env_vars
                .filter(|_| inst.operands.len() == 9)
                .filter(|value| matches!(ctx.value_php_type(*value), Ok(PhpType::Array(_) | PhpType::AssocArray { .. })));
            if let Some(environment) = dynamic_environment {
                ctx.load_value_to_result(environment)?;
                ctx.emitter.instruction("mov rdi, rax");                        // pass computed environment storage to the runtime marshaller
                abi::emit_call_label(ctx.emitter, "__rt_win_proc_environment");
                ctx.emitter.instruction("mov QWORD PTR [rsp + 72], rax");       // retain the owned environment block above staged ABI slots
                ctx.emitter.instruction("mov QWORD PTR [rsp + 88], rdx");       // preserve its counted byte length for packed flags
            } else {
                load_optional_proc_open_pointer(ctx, env_vars)?;
            }
            abi::emit_push_reg(ctx.emitter, "rax");
            load_optional_proc_open_pointer(ctx, packed_flags)?;
            if matches!(ctx.value_php_type(command)?, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                ctx.emitter.instruction("or rax, 1");                           // runtime command arrays always bypass cmd.exe wrapping
            }
            if dynamic_environment.is_some() {
                ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 104]");      // reload dynamic environment byte length after stacking its pointer
                ctx.emitter.instruction("cmp rdx, -1");                         // marshalling failure sentinel?
                let environment_valid = ctx.next_label("proc_open_environment_valid");
                let environment_flags_ready = ctx.next_label("proc_open_environment_flags_ready");
                ctx.emitter.instruction(&format!("jne {environment_valid}"));   // valid length can be packed normally
                ctx.emitter.instruction("bts rax, 63");                         // mark runtime ABI invalid while preserving helper errno
                ctx.emitter.instruction(&format!("jmp {environment_flags_ready}")); // skip length packing after failure
                ctx.emitter.label(&environment_valid);
                ctx.emitter.instruction("and rax, 31");                         // retain all five Windows proc_open option bits
                ctx.emitter.instruction("shl rdx, 5");                          // pack environment length above the option bits
                ctx.emitter.instruction("or rax, rdx");                         // combine environment length and Windows options
                ctx.emitter.label(&environment_flags_ready);
            }
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 112], 0");            // did dynamic command marshalling fail before flags were loaded?
            let command_flags_ready = ctx.next_label("proc_open_command_flags_ready");
            ctx.emitter.instruction(&format!("je {command_flags_ready}"));      // valid/static commands leave flags unchanged
            ctx.emitter.instruction("bts rax, 63");                             // propagate argv validation failure without overwriting errno
            ctx.emitter.label(&command_flags_ready);
            if let Some(options) = public_options.filter(|_| inst.operands.len() == 9) {
                if matches!(ctx.value_php_type(options)?, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                    abi::emit_push_reg(ctx.emitter, "rax");
                    ctx.load_value_to_result(options)?;
                    ctx.emitter.instruction("mov rdi, rax");                    // pass computed proc_open options to the runtime validator
                    abi::emit_call_label(ctx.emitter, "__rt_win_proc_options");
                    ctx.emitter.instruction("mov r10, rax");                    // retain the dynamic Windows option-bit mask
                    abi::emit_pop_reg(ctx.emitter, "rax");
                    ctx.emitter.instruction("cmp r10, -1");                     // did runtime option validation fail?
                    let options_valid = ctx.next_label("proc_open_options_valid");
                    ctx.emitter.instruction(&format!("jne {options_valid}"));   // valid false/true values continue normally
                    ctx.emitter.instruction("bts rax, 63");                     // mark the packed ABI invalid without losing errno
                    ctx.emitter.label(&options_valid);
                    ctx.emitter.instruction("or rax, r10");                     // merge recognized dynamic Windows options
                }
            }
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_pop_reg(ctx.emitter, "r11");
            abi::emit_pop_reg(ctx.emitter, "r10");
            abi::emit_pop_reg_pair(ctx.emitter, "r8", "r9");
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("sub rsp, 16");                             // reserve aligned SysV stack-argument slots 7 and 8
            ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                // pass environment storage after the six integer registers
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r11");            // pass packed flags after environment storage
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_proc_open");
    // Register the returned `$pipes` before boxing the process resource: the registry
    // retains the exact promoted container for proc_close's deadlock-safe cleanup.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_proc_pipe_registry_register");
            abi::emit_push_reg_pair(ctx.emitter, "x0", "x1");
            load_string_to_result(ctx, command, "proc_open status command")?;
            ctx.emitter.instruction("ldr x0, [sp]");                            // restore the raw process result as the status helper's first runtime argument
            abi::emit_call_label(ctx.emitter, "__rt_proc_status_register");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the raw process HANDLE to the pipe registry
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the paired final pipes container to the registry
            abi::emit_call_label(ctx.emitter, "__rt_proc_pipe_registry_register");
        }
    }
    if let Some(slot) = pipes_local {
        store_proc_open_pipes_result(ctx, slot)?;
    }
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("add rsp, 16");                                 // release the two optional stack-argument slots
        if matches!(ctx.value_php_type(command)?, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            if ctx.emitter.target.platform == Platform::Windows {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the raw process HANDLE to the Windows status registry
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");            // pass the still-owned marshalled argv command line
                abi::emit_call_label(ctx.emitter, "__rt_proc_status_register_cstr");
            } else {
                return Err(CodegenIrError::unsupported(
                    "proc_open() array commands require the Windows status registry",
                ));
            }
        } else {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the process descriptor while loading its source command
            load_string_to_result(ctx, command, "proc_open status command")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass the command pointer to the status registry
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // restore the raw process descriptor after string materialization
            abi::emit_call_label(ctx.emitter, "__rt_proc_status_register");
        }
        ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // preserve the process result across marshalling-buffer cleanup
        ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                    // load the optional owned dynamic command buffer
        ctx.emitter.instruction("test rax, rax");                               // did runtime argv marshalling allocate a command line?
        let skip_dynamic_command_free = ctx.next_label("proc_open_command_free_done");
        ctx.emitter.instruction(&format!("jz {skip_dynamic_command_free}"));    // static string commands own no staging buffer here
        abi::emit_call_label(ctx.emitter, "__rt_heap_free");
        ctx.emitter.label(&skip_dynamic_command_free);
        ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // load the optional owned environment block
        ctx.emitter.instruction("test rax, rax");                               // was a custom dynamic environment marshalled?
        let skip_dynamic_environment_free = ctx.next_label("proc_open_environment_free_done");
        ctx.emitter.instruction(&format!("jz {skip_dynamic_environment_free}")); // inherited/static environments own no block here
        abi::emit_call_label(ctx.emitter, "__rt_heap_free");
        ctx.emitter.label(&skip_dynamic_environment_free);
        ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // restore the process result
        ctx.emitter.instruction("add rsp, 48");                                 // release dynamic marshalling ownership state
    }
    box_stream_fd_or_false_result_kind(ctx, "proc_open", PROC_OPEN_CLEANUP_KIND);
    store_if_result(ctx, inst)
}

/// Stores the runtime's replacement `$pipes` array through the original by-ref local.
fn store_proc_open_pipes_result(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    let target_ty = ctx.local_php_type(slot)?.codegen_repr();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x0", "x1");
            ctx.release_local_before_refcounted_writeback(slot)?;
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x1");                              // present the returned pipes array to the established local-store path
            if target_ty == PhpType::Mixed {
                crate::codegen::emit_box_current_owned_value_as_mixed(
                    ctx.emitter,
                    &PhpType::Iterable,
                );
            }
            ctx.store_current_result_to_local(slot)?;
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.release_local_before_refcounted_writeback(slot)?;
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, rdx");                            // present the returned pipes array to the established local-store path
            if target_ty == PhpType::Mixed {
                crate::codegen::emit_box_current_owned_value_as_mixed(
                    ctx.emitter,
                    &PhpType::Iterable,
                );
            }
            ctx.store_current_result_to_local(slot)?;
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Loads an optional process string or the null/zero pair expected by the runtime.
fn load_optional_proc_open_string(
    ctx: &mut FunctionContext<'_>,
    value: Option<ValueId>,
    name: &str,
) -> Result<()> {
    let Some(value) = value else {
        zero_proc_open_string_result(ctx);
        return Ok(());
    };
    if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Void | PhpType::Never) {
        zero_proc_open_string_result(ctx);
        return Ok(());
    }
    load_string_to_result(ctx, value, name)
}

/// Clears the target string-result pair for an omitted optional process string.
fn zero_proc_open_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // absent optional string has no byte pointer
            ctx.emitter.instruction("mov x2, #0");                              // absent optional string has zero byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor eax, eax");                            // absent optional string has no byte pointer
            ctx.emitter.instruction("xor edx, edx");                            // absent optional string has zero byte length
        }
    }
}

/// Loads an optional process array/settings pointer or a null pointer when absent.
fn load_optional_proc_open_pointer(
    ctx: &mut FunctionContext<'_>,
    value: Option<ValueId>,
) -> Result<()> {
    let Some(value) = value else {
        zero_proc_open_pointer_result(ctx);
        return Ok(());
    };
    if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Void | PhpType::Never) {
        zero_proc_open_pointer_result(ctx);
        return Ok(());
    }
    ctx.load_value_to_result(value).map(|_| ())
}

/// Clears the integer result register for an omitted process-array setting.
fn zero_proc_open_pointer_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, #0"),                 // optional process pointer is null
        Arch::X86_64 => ctx.emitter.instruction("xor eax, eax"),                // optional process pointer is null
    }
}

/// Lowers `proc_close(process)` and marks a boxed resource as explicitly released.
pub(crate) fn lower_proc_close(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "proc_close", 1)?;
    let handle = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, handle)?;
    load_stream_fd_to_result(ctx, handle, "proc_close")?;
    apply_resource_release_sentinel(ctx, captured);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the process descriptor to the SysV-shaped runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_proc_close");
    store_if_result(ctx, inst)
}

/// Lowers non-consuming process-status lookup and boxes the returned status record.
pub(crate) fn lower_proc_get_status(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "proc_get_status", 1)?;
    let process = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, process, "proc_get_status")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the retained process descriptor to the SysV-shaped status helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_proc_get_status");
    box_stat_array_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `proc_terminate(process, signal = SIGTERM)` without consuming the resource.
pub(crate) fn lower_proc_terminate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "proc_terminate", 1, 2)?;
    let process = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, process, "proc_terminate")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            if let Some(signal) = inst.operands.get(1).copied() {
                load_proc_terminate_signal_as_int(ctx, signal, 16)?;
            } else {
                ctx.emitter.instruction("mov x0, #15");                         // PHP's default signal is SIGTERM
            }
            ctx.emitter.instruction("mov x1, x0");                              // pass the requested signal as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // preserve the process descriptor while materializing the signal
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the process descriptor across scalar coercion
            if let Some(signal) = inst.operands.get(1).copied() {
                load_proc_terminate_signal_as_int(ctx, signal, 16)?;
            } else {
                ctx.emitter.instruction("mov rax, 15");                         // PHP's default signal is SIGTERM
            }
            ctx.emitter.instruction("mov rsi, rax");                            // pass the requested signal to the SysV-shaped helper
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // restore the process descriptor as the first runtime argument
            ctx.emitter.instruction("add rsp, 16");                             // release the aligned process staging slot
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_proc_terminate");
    store_if_result(ctx, inst)
}

/// Coerces the weak PHP signal surface into the integer expected by the runtime.
fn load_proc_terminate_signal_as_int(
    ctx: &mut FunctionContext<'_>,
    signal: ValueId,
    process_staging_bytes: usize,
) -> Result<()> {
    match ctx.load_value_to_result(signal)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Str => {
            let invalid = ctx.next_label("proc_terminate_signal_type_error");
            let done = ctx.next_label("proc_terminate_signal_coerced");
            crate::codegen::lower_inst::enums::emit_string_result_to_int_checked(ctx, &invalid);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&invalid);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_release_temporary_stack(ctx.emitter, process_staging_bytes);
            let (message_label, message_len) = ctx.data.add_string(
                b"proc_terminate(): Argument #2 ($signal) must be of type int, string given",
            );
            crate::codegen::lower_inst::enums::emit_throw_static_type_error(
                ctx,
                &message_label,
                message_len,
            );
            ctx.emitter.label(&done);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "proc_terminate signal for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers `disk_free_space(path)` through the shared disk-space runtime helper.
pub(crate) fn lower_disk_free_space(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_disk_space(ctx, inst, "disk_free_space", 0)
}

/// Lowers `disk_total_space(path)` through the shared disk-space runtime helper.
pub(crate) fn lower_disk_total_space(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_disk_space(ctx, inst, "disk_total_space", 1)
}

/// Loads a path and disk-space mode into `__rt_disk_space`.
pub(super) fn lower_disk_space(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    mode: i64,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", mode);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the path pointer as the second disk-space argument
            abi::emit_load_int_immediate(ctx.emitter, "rdi", mode);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_disk_space");
    box_float_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `gethostname()` through the shared runtime helper.
pub(crate) fn lower_gethostname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "gethostname", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_gethostname");
    store_if_result(ctx, inst)
}

/// Lowers `gethostbyname(hostname)` through the shared runtime resolver.
pub(crate) fn lower_gethostbyname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "gethostbyname", 1)?;
    let host = expect_operand(inst, 0)?;
    load_string_to_result(ctx, host, "gethostbyname host")?;
    abi::emit_call_label(ctx.emitter, "__rt_gethostbyname");
    store_if_result(ctx, inst)
}

/// Lowers `gethostbyaddr(address)` and boxes malformed addresses as PHP `false`.
pub(crate) fn lower_gethostbyaddr(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "gethostbyaddr", 1)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "gethostbyaddr address")?;
    abi::emit_call_label(ctx.emitter, "__rt_gethostbyaddr");
    box_owned_string_or_false_result(ctx, "gethostbyaddr");
    store_if_result(ctx, inst)
}

/// Lowers `getprotobyname(protocol)` and boxes a missing entry as PHP `false`.
pub(crate) fn lower_getprotobyname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "getprotobyname", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "getprotobyname protocol")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the protocol byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the protocol byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_getprotobyname");
    box_negative_int_or_false_result(ctx, "getprotobyname");
    store_if_result(ctx, inst)
}

/// Lowers `getprotobynumber(number)` and boxes a missing entry as PHP `false`.
pub(crate) fn lower_getprotobynumber(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "getprotobynumber", 1)?;
    let protocol = expect_operand(inst, 0)?;
    require_int(
        ctx.load_value_to_result(protocol)?.codegen_repr(),
        "getprotobynumber number",
    )?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the protocol number as the runtime argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_getprotobynumber");
    box_owned_string_or_false_result(ctx, "getprotobynumber");
    store_if_result(ctx, inst)
}

/// Lowers `getservbyname(service, protocol)` and boxes a missing entry as PHP `false`.
pub(crate) fn lower_getservbyname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "getservbyname", 2)?;
    let service = expect_operand(inst, 0)?;
    let protocol = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, service, "getservbyname service")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, protocol, "getservbyname protocol")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the protocol pointer as the third runtime argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the protocol byte length as the fourth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, service, "getservbyname service")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, protocol, "getservbyname protocol")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the protocol byte length as the fourth runtime argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the protocol pointer as the third runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_getservbyname");
    box_negative_int_or_false_result(ctx, "getservbyname");
    store_if_result(ctx, inst)
}

/// Lowers `getservbyport(port, protocol)` and boxes a missing entry as PHP `false`.
pub(crate) fn lower_getservbyport(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "getservbyport", 2)?;
    let port = expect_operand(inst, 0)?;
    let protocol = expect_operand(inst, 1)?;
    require_int(
        ctx.load_value_to_result(port)?.codegen_repr(),
        "getservbyport port",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x0", "x0");
            load_string_to_result(ctx, protocol, "getservbyport protocol")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x9");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rax");
            load_string_to_result(ctx, protocol, "getservbyport protocol")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass the protocol pointer as the second runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_getservbyport");
    box_owned_string_or_false_result(ctx, "getservbyport");
    store_if_result(ctx, inst)
}

/// Lowers `opendir(path)` and boxes the directory stream as `resource|false`.
pub(crate) fn lower_opendir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "opendir", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "opendir path")?;
    abi::emit_call_label(ctx.emitter, "__rt_opendir");
    box_stream_fd_or_false_result_kind(ctx, "opendir", OPENDIR_CLEANUP_KIND);
    store_if_result(ctx, inst)
}

/// Lowers `readdir(dir_handle)` for libc, glob, and userspace-wrapper handles.
pub(crate) fn lower_readdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "readdir", 1)?;
    let handle = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, handle, "readdir")?;
    lower_directory_handle_dispatch(
        ctx,
        "__rt_readdir",
        "__rt_user_wrapper_dir_readdir",
        "readdir",
    );
    box_owned_string_or_false_result(ctx, "readdir");
    store_if_result(ctx, inst)
}

/// Lowers `closedir(dir_handle)` for libc, glob, and userspace-wrapper handles.
pub(crate) fn lower_closedir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "closedir", 1)?;
    let handle = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, handle)?;
    load_stream_fd_to_result(ctx, handle, "closedir")?;
    apply_resource_release_sentinel(ctx, captured);
    lower_directory_handle_dispatch(
        ctx,
        "__rt_closedir",
        "__rt_user_wrapper_dir_closedir",
        "closedir",
    );
    store_if_result(ctx, inst)
}

/// Lowers `rewinddir(dir_handle)` for libc, glob, and userspace-wrapper handles.
pub(crate) fn lower_rewinddir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "rewinddir", 1)?;
    let handle = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, handle, "rewinddir")?;
    lower_directory_handle_dispatch(
        ctx,
        "__rt_rewinddir",
        "__rt_user_wrapper_dir_rewinddir",
        "rewinddir",
    );
    store_if_result(ctx, inst)
}

/// Lowers `popen(command, mode)` and boxes the process pipe as `resource|false`.
pub(crate) fn lower_popen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "popen", 2)?;
    let command = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, command, "popen command")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, mode, "popen mode")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the mode pointer as the third runtime argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the mode byte length as the fourth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, command, "popen command")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, mode, "popen mode")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the mode byte length as the fourth runtime argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the mode pointer as the third runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_popen");
    box_stream_fd_or_false_result_kind(ctx, "popen", POPEN_CLEANUP_KIND);
    store_if_result(ctx, inst)
}

/// Lowers `pclose(handle)` and returns the child process status.
pub(crate) fn lower_pclose(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "pclose", 1)?;
    let handle = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, handle)?;
    load_stream_fd_to_result(ctx, handle, "pclose")?;
    apply_resource_release_sentinel(ctx, captured);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the pipe descriptor to the runtime close helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_pclose");
    store_if_result(ctx, inst)
}

/// Lowers `fsockopen(host, port, errno?, errstr?, timeout?)`.
pub(crate) fn lower_fsockopen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fsockopen", 2, 5)?;
    let host = expect_operand(inst, 0)?;
    let port = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, host, "fsockopen host")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(port)?.codegen_repr(), "fsockopen port")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            if inst.operands.len() >= 5 {
                let timeout = expect_operand(inst, 4)?;
                ctx.load_value_to_result(timeout)?;
            }
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, x1");                              // pass hostname pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass hostname byte length as the second runtime argument
            ctx.emitter.instruction("mov x2, x9");                              // pass TCP port as the third runtime argument
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, host, "fsockopen host")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(port)?.codegen_repr(), "fsockopen port")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            if inst.operands.len() >= 5 {
                let timeout = expect_operand(inst, 4)?;
                ctx.load_value_to_result(timeout)?;
            }
            abi::emit_pop_reg(ctx.emitter, "r8");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            ctx.emitter.instruction("mov rdx, r8");                             // pass TCP port as the third runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fsockopen");
    store_fsockopen_error_outputs(ctx, inst)?;
    box_stream_fd_or_false_result(ctx, "fsockopen");
    store_if_result(ctx, inst)
}

/// Lowers `file(path)` through the target-aware runtime line-array helper.
/// Lowers `file(path, flags)` through the target-aware runtime line-array helper.
///
/// PHP's `$flags` bitmask is an ordinary run-time integer, so it needs no literal: the helper
/// applies `FILE_IGNORE_NEW_LINES` / `FILE_SKIP_EMPTY_LINES` while it produces each line. The
/// flags are resolved and spilled BEFORE the path, because coercing a non-string path calls a
/// conversion helper that clobbers the caller-saved register the flags would otherwise sit in.
pub(crate) fn lower_file(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "file", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    match inst.operands.get(1).copied() {
        None => {
            load_string_to_result(ctx, path, "file")?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x0, #0");                      // no $flags argument: request PHP's default behavior
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("xor edi, edi");                    // no $flags argument: request PHP's default behavior
                }
            }
        }
        Some(flags) => {
            resolve_int_operand_to_result(ctx, flags, "file flags")?;
            abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
            load_string_to_result(ctx, path, "file")?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_pop_reg(ctx.emitter, "x0");                       // restore the resolved $flags bitmask into the first runtime argument
                }
                Arch::X86_64 => {
                    abi::emit_pop_reg(ctx.emitter, "rdi");                      // restore the resolved $flags bitmask into the first runtime argument
                }
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_file");
    box_indexed_array_or_false_result(ctx);                                     // a failed read is a null result, which PHP reports as false
    store_if_result(ctx, inst)
}

/// Lowers `realpath(path)` and boxes the owned runtime string-or-false result.
pub(crate) fn lower_realpath(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "realpath", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "realpath")?;
    abi::emit_call_label(ctx.emitter, "__rt_realpath");
    box_owned_string_or_false_result(ctx, "realpath");
    store_if_result(ctx, inst)
}

/// Lowers `realpath_cache_get()` to elephc's empty realpath-cache view.
pub(crate) fn lower_realpath_cache_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "realpath_cache_get", 0)?;
    emit_empty_mixed_hash(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `realpath_cache_size()` to zero because elephc has no realpath cache.
pub(crate) fn lower_realpath_cache_size(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "realpath_cache_size", 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                              // report an empty realpath cache size
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor rax, rax");                            // report an empty realpath cache size
        }
    }
    store_if_result(ctx, inst)
}
