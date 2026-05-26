//! Purpose:
//! Emits PHP `fileinode` filesystem metadata builtin calls.
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
use super::stat_result::box_stat_int_or_false_result;

/// Emits code for the PHP `fileinode(path)` builtin.
///
/// Evaluates `path` as the sole argument, calls the target-aware runtime helper
/// `__rt_fileinode` to retrieve the inode number via `stat`, then boxes the result
/// as a PHP `Mixed` value (either an integer inode or `false` on failure).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fileinode()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fileinode");                            // call the target-aware runtime helper that loads st_ino
    box_stat_int_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
