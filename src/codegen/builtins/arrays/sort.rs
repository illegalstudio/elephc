//! Purpose:
//! Emits PHP `sort` builtin calls that mutate array arguments in place.
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

/// Emits the PHP `sort()` builtin call, mutating the input array in place.
///
/// Inputs:
/// - `args[0]` must be an array-typed expression; it is evaluated for uniqueness and
///   its storage is marked as mutating so the caller sees the updated pointer.
/// - `_name` is unused (present for dispatcher signature compatibility).
///
/// Side effects:
/// - Calls `emit_ensure_unique_arg` to enforce COW before mutation.
/// - Calls `emit_store_mutating_arg` to preserve PHP-visible storage.
/// - Emits a call to `__rt_sort_int`, the target-aware runtime helper that sorts
///   indexed integer arrays in ascending order.
///
/// Returns:
/// - `Some(PhpType::Void)` indicating `sort()` has no return value in PHP.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sort()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- sort integer array in ascending order --
    abi::emit_call_label(emitter, "__rt_sort_int");                             // call the target-aware runtime helper that sorts indexed integer arrays ascending in place

    Some(PhpType::Void)
}
