//! Purpose:
//! Emits PHP `filegroup` filesystem metadata builtin calls.
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
use super::stat_result::box_stat_int_or_false_result;

/// Emits code for the PHP `filegroup()` builtin.
///
/// `filegroup()` returns the group ID of the file at `args[0]`, or `false` if
/// the file cannot be stat'd. The path expression is emitted first, then the
/// runtime helper `__rt_filegroup` is called to populate `st_gid` from the OS
/// stat buffer. The integer result (or `false` sentinel) is boxed into a
/// `PhpType::Mixed` and returned.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("filegroup()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_filegroup");                            // call the target-aware runtime helper that loads st_gid
    box_stat_int_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
