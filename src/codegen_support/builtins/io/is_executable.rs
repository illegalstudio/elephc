//! Purpose:
//! Emits PHP `is_executable` filesystem metadata builtin calls.
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

/// Emits `is_executable($path)` by evaluating the path argument, calling the
/// `__rt_is_executable` runtime helper, and returning `PhpType::Bool`.
///
/// Expects exactly one argument (the path expression). The runtime helper
/// performs `access(path, X_OK)` on the target platform. Filesystem state is
/// observable, so call order and failure sentinels must be preserved.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_executable()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_is_executable");                        // call the target-aware runtime helper that runs access(path, X_OK)
    Some(PhpType::Bool)
}
