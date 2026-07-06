//! Purpose:
//! Emits PHP `is_link` filesystem metadata builtin calls.
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

/// Emits the `is_link` builtin call for a single path argument.
///
/// # Arguments
/// - `_name`: Unused; always `is_link` (case-insensitive lookup handled by catalog).
/// - `args`: Exactly one expression yielding a path string.
/// - `emitter`: Assembly emitter for the current function.
/// - `ctx`: Codegen context (variable layout, class metadata).
/// - `data`: Data section for relocations and static data.
///
/// # Behavior
/// Evaluates `args[0]` (path argument), then calls `__rt_is_link` which invokes
/// `lstat()` and checks `S_ISLNK`. Returns `PhpType::Bool` (PHP `is_link` is always bool).
///
/// # Safety & invariants
/// - Filesystem state is observable; call order must match source evaluation order.
/// - The runtime helper handles platform-specific `S_ISLNK` detection.
/// - Result is always `Some(PhpType::Bool)`; no failure sentinel — PHP false on error.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_link()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_is_link");                              // call the target-aware runtime helper that runs lstat() and checks S_ISLNK
    Some(PhpType::Bool)
}
