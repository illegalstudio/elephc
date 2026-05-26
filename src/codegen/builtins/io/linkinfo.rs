//! Purpose:
//! Emits PHP `linkinfo` builtin calls.
//! Returns the `st_dev` field of the link (or -1 on failure).
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The runtime helper invokes libc `lstat()` and returns the platform `st_dev`
//!   field on success, or PHP's `-1` failure sentinel.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `linkinfo()` builtin.
///
/// `linkinfo()` calls the runtime helper `__rt_linkinfo`, which wraps libc
/// `lstat()` and returns the `st_dev` field of the link as an integer.
/// Returns `-1` on failure (e.g., if the path does not exist or is not a symlink).
///
/// # Arguments
/// * `_name` - Unused; present for dispatcher uniformity.
/// * `args` - Must contain exactly one argument: the path as a string expression.
/// * `emitter` - Target assembly emitter.
/// * `ctx` - Codegen context (variable layout, ownership).
/// * `data` - Data section for string literals and metadata.
///
/// # Returns
/// Always returns `PhpType::Int` (the device ID or -1 on failure).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("linkinfo()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_linkinfo");                             // libc lstat() wrapper that returns the device id
    Some(PhpType::Int)
}
