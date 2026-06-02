//! Purpose:
//! Emits PHP `mkdir` filesystem mutation builtin calls.
//! Routes `scheme://` paths matching a registered userspace wrapper to the
//! wrapper's `mkdir()` method; all other paths use the libc `__rt_mkdir`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.
//! - The wrapper split mirrors `readfile()`: a `__rt_path_is_wrapper` probe picks
//!   the wrapper branch (`__rt_user_wrapper_path_op` with the `mkdir` vtable slot
//!   17) over the libc filesystem branch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::path_op_wrapper::emit_single_path_wrapper_dispatch;

/// `mkdir` vtable slot index in the per-class user-wrapper vtable.
const MKDIR_SLOT: usize = 17;

/// Emits code for the PHP `mkdir(path, ...)` builtin.
///
/// Arguments:
///   - `args[0]`: path expression, emitted via `emit_expr` before the dispatch.
///   - `_name`: unused; preserved for dispatcher signature parity.
///   - `ctx`, `data`: carried through to `emit_expr` for path materialization.
///
/// Returns: `Some(PhpType::Bool)` — PHP `mkdir` returns `bool` on success/failure.
///
/// Runtime contract: a registered `scheme://` path dispatches to the wrapper's
/// `mkdir()` (vtable slot 17) via `__rt_user_wrapper_path_op`; any other path
/// calls the libc `__rt_mkdir`. (v1: `$mode`/`$recursive` are not threaded — the
/// libc path uses mode `0755`; the wrapper receives zeroed extra arguments.)
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("mkdir()");
    emit_expr(&args[0], emitter, ctx, data);
    emit_single_path_wrapper_dispatch(emitter, ctx, "__rt_mkdir", MKDIR_SLOT);
    Some(PhpType::Bool)
}
