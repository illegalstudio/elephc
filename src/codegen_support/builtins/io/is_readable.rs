//! Purpose:
//! Emits PHP `is_readable` filesystem metadata builtin calls.
//! Delegates platform stat work to runtime helpers and boxes PHP false-or-value results.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Filesystem state is observable, so emitters must preserve call order and failure sentinels.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to PHP's `is_readable()` builtin.
///
/// Evaluates `args[0]` as a filesystem path expression and calls `__rt_is_readable`
/// to check whether the path is readable by the current process.
///
/// # Arguments
/// - `args[0]`: path expression to check
///
/// # Returns
/// `Some(PhpType::Bool)` — the result of the readability check
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_readable()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_is_readable");                          // call the target-aware runtime helper that checks whether the path is readable
    Some(PhpType::Bool)
}
