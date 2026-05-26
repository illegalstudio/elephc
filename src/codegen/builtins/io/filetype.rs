//! Purpose:
//! Emits PHP `filetype` filesystem metadata builtin calls.
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
use super::stat_result::box_stat_string_or_false_result;

/// Emits code for the PHP `filetype` builtin.
///
/// Evaluates the path argument, calls `__rt_filetype` to retrieve the filesystem
/// type string (`"file"`, `"dir"`, `"link"`, etc.), boxes the result, and returns
/// `PhpType::Mixed`. On failure (e.g., file not found), emits a PHP `false` sentinel.
///
/// - **args**: must contain exactly one path expression (checked by caller).
/// - **emitter**: receives the evaluated path load, call to `__rt_filetype`, and box result.
/// - **ctx**: carries variable layout and ownership state through codegen.
/// - **data**: accumulates data-section entries for string literals and metadata.
/// - **Returns**: `Some(PhpType::Mixed)` unconditionally; caller relies on runtime result.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("filetype()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_filetype");                             // call the target-aware runtime helper that returns "file"/"dir"/"link"/...
    box_stat_string_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
