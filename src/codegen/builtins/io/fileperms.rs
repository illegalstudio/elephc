//! Purpose:
//! Emits PHP `fileperms` filesystem metadata builtin calls.
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

/// Emits the `fileperms` builtin call.
///
/// Evaluates the file path argument, calls the `__rt_fileperms` runtime helper
/// which invokes `stat()` and extracts the `st_mode` field, then boxes the result
/// as `PhpType::Mixed` (integer permission mask on success, PHP false on failure).
///
/// # Arguments
/// - `_name`: unused, follows the builtin emitter convention
/// - `args[0]`: the file path expression
///
/// # Returns
/// Always returns `Some(PhpType::Mixed)` — the boxed result is never consumed by a caller
/// that would interpret `None` as an error; the PHP false sentinel handles failure.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fileperms()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fileperms");                            // call the target-aware runtime helper that loads st_mode
    box_stat_int_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
