//! Purpose:
//! Emits PHP `scandir` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `scandir` builtin call.
///
/// First argument (path) is evaluated and emitted as an expression. Then calls the
/// `__rt_scandir` runtime helper which enumerates directory entries and returns them
/// as a string array. On failure (e.g., invalid path, not a directory), runtime returns
/// `false` rather than an array — callers must handle false-on-failure semantics.
///
/// # Arguments
/// * `_name` - Unused; present for dispatcher uniformity with other builtin emitters.
/// * `args` - Must contain at least a path expression as the first element.
/// * `emitter` - Target-aware assembly emitter.
/// * `ctx` - Codegen context carrying variable layout and metadata.
/// * `data` - Data section for relocations and static storage.
///
/// # Returns
/// `Some(PhpType::Array(Box::new(PhpType::Str)))` — callers should treat `false`
/// from runtime as the actual failure indicator.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("scandir()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_scandir");                              // call the target-aware runtime helper that lists directory entries into a string array
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
