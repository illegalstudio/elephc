//! Purpose:
//! Emits PHP `file_exists` filesystem metadata builtin calls.
//! Delegates platform stat work to runtime helpers and boxes PHP false-or-value results.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Filesystem state is observable, so emitters must preserve call order and failure sentinels.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `file_exists` filesystem check for a single path argument.
///
/// Evaluates the path expression (argument 0), then calls `__rt_file_exists`
/// to perform the target-aware stat. Returns `PhpType::Bool` indicating whether
/// the path exists; the runtime helper handles all platform-specific logic.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("file_exists()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_file_exists");                          // call the target-aware runtime helper that checks whether the path exists
    Some(PhpType::Bool)
}
