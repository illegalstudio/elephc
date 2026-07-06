//! Purpose:
//! Emits PHP `shell_exec` process-control or shell execution builtin calls.
//! Marshals command/status arguments into runtime helpers with PHP-visible output and exit behavior.
//!
//! Called from:
//! - `crate::codegen_support::builtins::system::emit()`.
//!
//! Key details:
//! - Process calls are effectful and may terminate or emit output, so lowering must preserve evaluation order.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `shell_exec` builtin calls.
///
/// # Arguments
/// - `_name`: Unused; the builtin name is hardcoded as `shell_exec`.
/// - `args`: Single argument — the command string to execute.
///
/// # Behavior
/// Evaluates the command string argument in source order, then calls the runtime
/// helper `__rt_shell_exec` to execute the command and capture stdout as a string.
/// Returns `PhpType::Str` as the captured output.
///
/// # ABI
/// The runtime call uses target-aware ABI helpers to materialize arguments and
/// capture the ptr/len result in registers per the target convention.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("shell_exec()");
    // -- evaluate command string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call runtime to execute command and capture output --
    abi::emit_call_label(emitter, "__rt_shell_exec");                           // execute command via the target-aware shell helper → ptr/len result regs
    Some(PhpType::Str)
}
