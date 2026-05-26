//! Purpose:
//! Emits PHP `fileowner` filesystem metadata builtin calls.
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

/// Emits the `fileowner` builtin call.
///
/// Evaluates the path argument, calls the runtime helper that retrieves `st_uid`
/// via the target-aware stat path, boxes the integer UID or `false` into a PHP
/// `Mixed` value, and returns `PhpType::Mixed`.
///
/// Arguments:
/// - `args[0]` must be a path expression (string).
///
/// Side effects: filesystem state is observable; call order and failure sentinels are preserved.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fileowner()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fileowner");                            // call the target-aware runtime helper that loads st_uid
    box_stat_int_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
