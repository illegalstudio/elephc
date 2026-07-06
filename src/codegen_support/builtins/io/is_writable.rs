//! Purpose:
//! Emits PHP `is_writable` filesystem metadata builtin calls.
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

/// Emits the `is_writable` builtin call.
///
/// # Arguments
/// - `_name`: Unused name matching the builtin catalog entry.
/// - `args`: Single argument supplying the filesystem path to check.
/// - `emitter`: Target assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and scope.
/// - `data`: Data section for relocations and static data.
///
/// # Returns
/// Always returns `Some(PhpType::Bool)` since `is_writable` is a predicate.
///
/// # Codegen behavior
/// Emits the path argument expression, then calls `__rt_is_writable` to perform
/// the platform-specific stat operation. The result is a PHP boolean.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_writable()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_is_writable");                          // call the target-aware runtime helper that checks whether the path is writable
    Some(PhpType::Bool)
}
