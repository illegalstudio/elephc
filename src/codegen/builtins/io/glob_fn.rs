//! Purpose:
//! Emits PHP `glob` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for the PHP `glob()` builtin.
///
/// Evaluates the pattern argument, then calls `__rt_glob` to expand the glob pattern
/// into an array of matching file paths. Returns `Array<Str>` on success, or `false`
/// on failure (handled by the runtime helper's false-on-failure return convention).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("glob()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_glob");                                 // call the target-aware runtime helper that expands the glob pattern into a string array
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
