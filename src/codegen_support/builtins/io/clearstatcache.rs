//! Purpose:
//! Emits PHP `clearstatcache` calls for filesystem metadata cache state.
//! Provides the codegen hook even when runtime cache behavior is minimal.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The call is effectful from PHP's perspective and should remain ordered with stat-family operations.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `clearstatcache` builtin call.
///
/// In elefhc this is a no-op: there is no filesystem metadata cache to clear.
/// Arguments are still evaluated (for side effects) and discarded.
///
/// # Arguments
/// * `_name` — Unused builtin name (passed by the dispatcher).
/// * `args` — Any supplied arguments; each is emitted to consume its side effects.
/// * `emitter` — Assembly emitter.
/// * `ctx` — Codegen context (types, locals, etc.).
/// * `data` — Data section for read-only constants.
///
/// # Returns
/// Always `PhpType::Void`. The call itself produces no value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("clearstatcache() — no-op (elephc has no stat cache)");
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    Some(PhpType::Void)
}
