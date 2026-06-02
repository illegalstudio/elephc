//! Purpose:
//! Emits PHP `unlink` filesystem mutation builtin calls.
//! Routes `scheme://` paths matching a registered userspace wrapper to the
//! wrapper's `unlink()` method; all other paths use the libc `__rt_unlink`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.
//! - The wrapper split mirrors `readfile()`: a `__rt_path_is_wrapper` probe picks
//!   the wrapper branch (`__rt_user_wrapper_path_op` with the `unlink` vtable
//!   slot 15) over the filesystem branch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::path_op_wrapper::emit_single_path_wrapper_dispatch;

/// `unlink` vtable slot index in the per-class user-wrapper vtable.
const UNLINK_SLOT: usize = 15;

/// Emits code for the PHP `unlink(path)` builtin.
/// Consumes the path argument, dispatches a registered `scheme://` path to the
/// wrapper's `unlink()` (vtable slot 15) and any other path to the libc helper
/// `__rt_unlink`, then returns a bool (true on success, false on failure).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("unlink()");
    emit_expr(&args[0], emitter, ctx, data);
    emit_single_path_wrapper_dispatch(emitter, ctx, "__rt_unlink", UNLINK_SLOT);
    Some(PhpType::Bool)
}
