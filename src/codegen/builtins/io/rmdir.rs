//! Purpose:
//! Emits PHP `rmdir` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `rmdir` PHP builtin call.
///
/// Arguments (evaluated left-to-right):
/// - `args[0]`: path string to the directory to remove
/// - `args[1]`: context resource (ignored, reserved for stream context)
///
/// Calls `__rt_rmdir` with the path pointer/length in x1/x2, then returns bool.
/// Sets errno and returns false if the directory cannot be removed (not empty, permissions, etc.).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rmdir()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_rmdir");                                // call the target-aware runtime helper that removes an empty directory path
    Some(PhpType::Bool)
}
