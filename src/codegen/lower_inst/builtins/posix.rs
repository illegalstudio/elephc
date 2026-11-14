//! Purpose:
//! Lowers the typed ext/posix operations that need more than a plain bridge call:
//! `posix_kill()`'s process-id `ValueError` and `posix_isatty()`'s int-or-stream operand.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::pcntl::lower()` for `PcntlRuntime::PosixKill`
//!   and `PcntlRuntime::PosixIsatty`.
//!
//! Key details:
//! - Both bridges return an `i64` status whose negative value selects PHP's diagnostic.
//! - A stream resource is reduced to its native descriptor before the terminal probe; a boxed
//!   operand is inspected at run time so `int|resource` values take the matching path.
//! - A closed stream (negative release sentinel) throws PHP's `TypeError` before the probe.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::strings::load_as_int;
use super::{ensure_arg_count, expect_operand, store_if_result};

/// Runtime `Mixed` tag carried by boxed resource values.
const MIXED_TAG_RESOURCE: i64 = 9;

/// Lowers `posix_kill()` and raises PHP's `ValueError` for a process id outside `pid_t`.
pub(super) fn lower_kill(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "posix_kill", 2)?;
    load_as_int(ctx, expect_operand(inst, 0)?, "posix_kill process_id")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_as_int(ctx, expect_operand(inst, 1)?, "posix_kill signal")?;
    let invalid = ctx.next_label("posix_kill_invalid_pid");
    let done = ctx.next_label("posix_kill_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- deliver the signal and branch to the ValueError on a pid outside pid_t --
            ctx.emitter.instruction("mov x1, x0");                              // C arg1 = signal number to deliver
            abi::emit_pop_reg(ctx.emitter, "x0");                              // C arg0 = target process or group id
            ctx.emitter.bl_c("elephc_posix_kill");
            ctx.emitter.instruction("cmp x0, #0");                              // a negative status flags an out-of-range process id
            ctx.emitter.instruction(&format!("b.lt {invalid}"));                // raise PHP's ValueError instead of returning
        }
        Arch::X86_64 => {
            // -- deliver the signal and branch to the ValueError on a pid outside pid_t --
            ctx.emitter.instruction("mov rsi, rax");                            // C arg1 = signal number to deliver
            abi::emit_pop_reg(ctx.emitter, "rdi");                             // C arg0 = target process or group id
            ctx.emitter.bl_c("elephc_posix_kill");
            ctx.emitter.instruction("cmp rax, 0");                              // a negative status flags an out-of-range process id
            ctx.emitter.instruction(&format!("jl {invalid}"));                  // raise PHP's ValueError instead of returning
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&invalid);
    super::super::exceptions::emit_value_error(ctx, elephc_pcntl::POSIX_KILL_PID_RANGE_MESSAGE);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `posix_isatty()` for an integer descriptor, a stream resource, or a boxed value.
pub(super) fn lower_isatty(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "posix_isatty", 1)?;
    let operand = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(operand)? {
        PhpType::Resource(_) => {
            ctx.load_value_to_result(operand)?;
            emit_reject_closed_stream(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => emit_boxed_descriptor(ctx, operand)?,
        _ => load_as_int(ctx, operand, "posix_isatty file_descriptor")?,
    }
    let done = ctx.next_label("posix_isatty_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- probe the descriptor and skip the range warning on a boolean status --
            ctx.emitter.bl_c("elephc_posix_isatty");
            ctx.emitter.instruction("cmp x0, #0");                              // a negative status flags an out-of-range descriptor
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // terminal and non-terminal results are already booleans
        }
        Arch::X86_64 => {
            // -- probe the descriptor and skip the range warning on a boolean status --
            ctx.emitter.instruction("mov rdi, rax");                            // C arg0 = descriptor to probe
            ctx.emitter.bl_c("elephc_posix_isatty");
            ctx.emitter.instruction("cmp rax, 0");                              // a negative status flags an out-of-range descriptor
            ctx.emitter.instruction(&format!("jge {done}"));                    // terminal and non-terminal results are already booleans
        }
    }
    emit_fd_range_warning(ctx);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Loads a boxed `int|resource` operand as a descriptor, unwrapping resources to their fd.
///
/// Non-resource payloads fall back to PHP's weak integer conversion of the same operand.
fn emit_boxed_descriptor(
    ctx: &mut FunctionContext<'_>,
    operand: crate::ir::ValueId,
) -> Result<()> {
    let scalar = ctx.next_label("posix_isatty_scalar");
    let loaded = ctx.next_label("posix_isatty_fd_loaded");
    ctx.load_value_to_result(operand)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- unwrap a boxed stream resource to its native descriptor --
            ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_RESOURCE}")); // does the boxed operand hold a stream resource?
            ctx.emitter.instruction(&format!("b.ne {scalar}"));                 // integers and other scalars use weak int conversion
            ctx.emitter.instruction("mov x0, x1");                              // the resource payload is its native descriptor
        }
        Arch::X86_64 => {
            // -- unwrap a boxed stream resource to its native descriptor --
            ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_RESOURCE}")); // does the boxed operand hold a stream resource?
            ctx.emitter.instruction(&format!("jne {scalar}"));                  // integers and other scalars use weak int conversion
            ctx.emitter.instruction("mov rax, rdi");                            // the resource payload is its native descriptor
        }
    }
    emit_reject_closed_stream(ctx);
    abi::emit_jump(ctx.emitter, &loaded);
    ctx.emitter.label(&scalar);
    load_as_int(ctx, operand, "posix_isatty file_descriptor")?;
    ctx.emitter.label(&loaded);
    Ok(())
}

/// Throws PHP's `TypeError` when the stream descriptor in the int result register is closed.
///
/// `fclose()` stamps a negative release sentinel into the resource payload, so a negative
/// descriptor here can only be a closed stream, never a caller-supplied integer.
fn emit_reject_closed_stream(ctx: &mut FunctionContext<'_>) {
    let open = ctx.next_label("posix_isatty_stream_open");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- let open streams through; closed ones fall into the TypeError --
            ctx.emitter.instruction("cmp x0, #0");                              // a negative payload is fclose()'s release sentinel
            ctx.emitter.instruction(&format!("b.ge {open}"));                   // open streams keep their native descriptor
        }
        Arch::X86_64 => {
            // -- let open streams through; closed ones fall into the TypeError --
            ctx.emitter.instruction("cmp rax, 0");                              // a negative payload is fclose()'s release sentinel
            ctx.emitter.instruction(&format!("jge {open}"));                    // open streams keep their native descriptor
        }
    }
    super::super::exceptions::emit_type_error(ctx, elephc_pcntl::POSIX_ISATTY_CLOSED_STREAM_MESSAGE);
    ctx.emitter.label(&open);
}

/// Emits PHP's out-of-range descriptor warning through the shared diagnostic channel.
fn emit_fd_range_warning(ctx: &mut FunctionContext<'_>) {
    let (label, len) = ctx.data.add_string(elephc_pcntl::POSIX_ISATTY_FD_RANGE_WARNING.as_bytes());
    let (pointer_reg, length_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_symbol_address(ctx.emitter, pointer_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, length_reg, len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}
