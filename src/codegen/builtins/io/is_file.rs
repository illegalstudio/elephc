//! Purpose:
//! Emits PHP `is_file` filesystem metadata builtin calls.
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

/// Emits a call to the `is_file` builtin.
///
/// # Arguments
/// - `args[0]`: the path expression to check (evaluated and passed to the runtime helper)
/// - `_name`: unused; matches the dispatcher signature
///
/// # Output
/// Emits the path argument, calls `__rt_is_file`, and returns `Some(PhpType::Bool)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_file()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_is_file");                              // call the target-aware runtime helper that checks whether the path is a regular file
    Some(PhpType::Bool)
}
