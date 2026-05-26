//! Purpose:
//! Emits PHP `exec` process-control or shell execution builtin calls.
//! Marshals command/status arguments into runtime helpers with PHP-visible output and exit behavior.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Process calls are effectful and may terminate or emit output, so lowering must preserve evaluation order.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the `exec()` builtin, which executes a shell command and returns its output.
/// Takes a command string expression, evaluates it, then calls `__rt_shell_exec` to run the command.
/// Returns the captured output as a string (last line of stdout) via x1=ptr, x2=len.
/// This is effectful: execution may emit output, terminate the process, or produce side effects.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("exec()");
    // -- evaluate command string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call runtime to execute command and capture output --
    abi::emit_call_label(emitter, "__rt_shell_exec");                           // execute command via the target-aware shell helper → ptr/len result regs
    // exec() returns the last line of output (same as shell_exec for simplicity)
    Some(PhpType::Str)
}
