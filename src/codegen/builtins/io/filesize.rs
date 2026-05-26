//! Purpose:
//! Emits PHP `filesize` filesystem metadata builtin calls.
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

/// Emits PHP `filesize()` call: evaluates the filename argument, calls the
/// platform-aware runtime helper `__rt_filesize`, and returns the file size in
/// bytes as `PhpType::Int` on success.
///
/// # Arguments
/// - `_name`: Unused placeholder matching the builtin dispatcher signature.
/// - `args`: Single argument expression producing the filename (string).
/// - `emitter`: Assembly emitter for the current function.
/// - `ctx`: Codegen context carrying target, variable layout, and function metadata.
/// - `data`: Data section for relocations and read-only literals.
///
/// # Returns
/// `Some(PhpType::Int)` unconditionally; PHP `false` on error is returned by
/// the runtime helper via the scalar result convention (not via this return value).
///
/// # Side effects
/// Emits a `bl __rt_filesize` call. The runtime helper performs a `stat`-style
/// syscall, making filesystem state observable. Call order is preserved.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("filesize()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_filesize");                             // call the target-aware runtime helper that returns the file size in bytes
    Some(PhpType::Int)
}
