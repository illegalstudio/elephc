//! Purpose:
//! Emits PHP `fwrite` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits a PHP `fwrite` call by unboxing the stream resource to a raw file descriptor,
/// evaluating the data string expression, then invoking the platform `write` syscall
/// (ARM64) or libc `write()` function (X86_64). The file descriptor is saved across
/// the data expression evaluation to avoid register conflicts.
///
/// # Arguments
/// * `_name` — unused, matches the builtin dispatcher signature
/// * `args[0]` — stream resource; must be a valid open file handle
/// * `args[1]` — string data to write
/// * `emitter` — target-specific assembly emitter
/// * `ctx` — codegen context (used by `emit_stream_fd_arg`)
/// * `data` — data section for relocations and constants
///
/// # Returns
/// Always `Some(PhpType::Int)` (bytes written), matching PHP `fwrite` semantics.
///
/// # Platform behavior
/// * ARM64: pushes fd to stack, evaluates data into x0, restores fd from stack, invokes syscall 4
/// * X86_64: preserves fd in rax, evaluates data into rax, moves to rdi/rsi for libc write()
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fwrite()");
    emit_stream_fd_arg("fwrite", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // push the file descriptor while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the file descriptor into the first __rt_fwrite argument register
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the payload, applying any attached write filter
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the file descriptor while the data expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first __rt_fwrite argument register
            emitter.instruction("mov rsi, rax");                                // move the elephc string pointer into the second __rt_fwrite argument register
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the payload, applying any attached write filter
        }
    }
    Some(PhpType::Int)
}
