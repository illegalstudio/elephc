//! Purpose:
//! Emits PHP `fileatime` filesystem metadata builtin calls.
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

/// Emits code for the PHP `fileatime()` builtin, which returns the last access time of a file.
///
/// # Arguments
/// - `_name`: Unused parameter present for dispatcher uniformity.
/// - `args`: Single argument providing the file path expression.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and metadata.
/// - `data`: Data section for relocatable literals.
///
/// # Returns
/// Always returns `Some(PhpType::Mixed)` because the builtin can return an integer
/// timestamp on success or `false` on failure.
///
/// # Behavior
/// 1. Emits code to evaluate and push the file path argument.
/// 2. Calls `__rt_fileatime`, the target-aware runtime helper that invokes `stat` and
///    extracts `st_atime`.
/// 3. Boxes the raw integer or `false` sentinel into a PHP `Mixed` value.
///
/// # Notes
/// Filesystem state is observable; emitters must preserve call order and propagate
/// the `false` sentinel on failure rather than raising a fatal error.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fileatime()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fileatime");                            // call the target-aware runtime helper that loads st_atime
    box_stat_int_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
