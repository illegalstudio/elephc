//! Purpose:
//! Emits PHP `is_dir` filesystem metadata builtin calls.
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

/// Emits x86_64 / ARM64 codegen for PHP's `is_dir(path)` builtin.
///
/// Evaluates `path` (a string expression), calls the runtime helper
/// `__rt_is_dir`, and returns a `bool`. The runtime helper performs
/// a target-aware stat call and signals failure via a sentinel value
/// rather than panicking, preserving PHP semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_dir()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_is_dir");                               // call the target-aware runtime helper that checks whether the path is a directory
    Some(PhpType::Bool)
}
