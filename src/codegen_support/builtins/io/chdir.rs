//! Purpose:
//! Emits PHP `chdir` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the PHP `chdir()` builtin, which changes the current working directory.
///
/// ## Inputs
/// - `name`: unused (placeholder for dispatcher compatibility).
/// - `args[0]`: the directory path expression; emitted before the runtime call.
/// - `emitter`, `ctx`, `data`: standard codegen state for expression emission and data section.
///
/// ## Outputs
/// Always returns `Some(PhpType::Bool)` — PHP `chdir()` returns a boolean indicating success.
///
/// ## Runtime behavior
/// Calls the target-aware `__rt_chdir` runtime helper, which performs an observable OS
/// filesystem mutation. The path argument is evaluated first (with observable side effects
/// in source order), then the runtime helper is invoked. The helper returns 1 (true) on
/// success or 0 (false) on failure, mirroring PHP semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("chdir()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_chdir");                                // call the target-aware runtime helper that changes the current working directory
    Some(PhpType::Bool)
}
