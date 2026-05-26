//! Purpose:
//! Emits PHP `lstat` filesystem metadata builtin calls.
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
use super::stat_result::box_stat_array_or_false_result;

/// Emits code for the PHP `lstat()` builtin, which returns filesystem metadata
/// for a file without following symlinks.
///
/// # Arguments
/// - `args[0]`: the file path (string expression)
/// - `emitter`: assembly emitter
/// - `ctx`: codegen context (for variable layout, ownership state)
/// - `data`: data section (for string/array literals)
///
/// # Returns
/// `Some(PhpType::Mixed)` — `lstat` always produces a value (array on success, `false` on failure).
///
/// # Runtime behavior
/// Calls `__rt_lstat_array` to build a PHP-compatible metadata array or emit `false`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("lstat()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_lstat_array");                          // call the target-aware runtime helper that builds the PHP-compatible lstat array
    box_stat_array_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
