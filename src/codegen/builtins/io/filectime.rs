//! Purpose:
//! Emits PHP `filectime` filesystem metadata builtin calls.
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

/// Emits a call to PHP `filectime($path)`.
///
/// Input: `args[0]` must be a path expression (string). Emitter evaluates it and
/// passes the resulting string pointer+length to `__rt_filectime` via the ABI.
///
/// Output: returns `Some(PhpType::Mixed)` — the boxed i64 modification timestamp
/// on success, or boxed PHP `false` on failure (including non-existent paths).
///
/// Side effects: calls `__rt_filectime` runtime helper; observable filesystem access
/// means call order and error handling must match PHP semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("filectime()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_filectime");                            // call the target-aware runtime helper that loads st_ctime
    box_stat_int_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
