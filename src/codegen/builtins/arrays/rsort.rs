//! Purpose:
//! Emits PHP `rsort` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use crate::codegen::abi;
use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the PHP `rsort` builtin, mutating the first argument array in place
/// in descending order. COW is handled via `emit_ensure_unique_arg` before the call, and
/// any replacement array pointer is written back to caller storage via
/// `emit_store_mutating_arg` after the call.
///
/// # Arguments
/// - `_name`: unused, matches the `BuiltinDef` dispatcher signature
/// - `args`: first arg is the array to sort; additional args (flags) are currently unused
/// - `emitter`, `ctx`, `data`: standard codegen context
///
/// # Returns
/// `Some(PhpType::Void)` since rsort has no meaningful return value for assignment
///
/// # Runtime behavior
/// Calls `__rt_rsort_int` to sort indexed integer arrays descending in place; caller
/// must ensure no value-temp preevaluation occurs for mutating/ref-like arguments.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rsort()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- sort the array in place, dispatching on the element family --
    let sort_runtime = match &arr_ty {
        PhpType::Array(elem) if matches!(**elem, PhpType::Str) => "__rt_rsort_str",
        _ => "__rt_rsort_int",
    };
    abi::emit_call_label(emitter, sort_runtime); // sort the indexed array descending in place (string- or integer-aware)

    Some(PhpType::Void)
}
