//! Purpose:
//! Emits PHP `rmdir` filesystem mutation builtin calls.
//! Routes `scheme://` paths matching a registered userspace wrapper to the
//! wrapper's `rmdir()` method; all other paths use the libc `__rt_rmdir`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.
//! - The wrapper split mirrors `readfile()`: a `__rt_path_is_wrapper` probe picks
//!   the wrapper branch (`__rt_user_wrapper_path_op` with the `rmdir` vtable slot
//!   18) over the libc filesystem branch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::path_op_wrapper::emit_single_path_wrapper_dispatch;

/// `rmdir` vtable slot index in the per-class user-wrapper vtable.
const RMDIR_SLOT: usize = 18;

/// Emits the `rmdir` PHP builtin call.
///
/// Arguments (evaluated left-to-right):
/// - `args[0]`: path string to the directory to remove
/// - `args[1]`: context resource (ignored, reserved for stream context)
///
/// A registered `scheme://` path dispatches to the wrapper's `rmdir()` (vtable
/// slot 18) via `__rt_user_wrapper_path_op`; any other path calls the libc
/// `__rt_rmdir` (path in x1/x2 on AArch64, rax/rdx on x86_64). Returns bool;
/// false on failure (not empty, permissions, wrapper miss, etc.).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rmdir()");
    emit_expr(&args[0], emitter, ctx, data);
    emit_single_path_wrapper_dispatch(emitter, ctx, "__rt_rmdir", RMDIR_SLOT);
    Some(PhpType::Bool)
}
