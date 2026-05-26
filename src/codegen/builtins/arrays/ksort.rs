//! Purpose:
//! Emits PHP `ksort` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the `ksort` runtime helper, sorting an associative array by keys in-place.
///
/// # Arguments
/// - `_name`: Unused name parameter (present for dispatcher uniformity).
/// - `args`: Must contain at least the array expression to sort. Additional arguments (e.g., `SORT_REGULAR`) are currently ignored.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context carrying variable layout and ownership state.
/// - `data`: Data section for relocations and static data.
///
/// # Returns
/// `Some(PhpType::Void)` — `ksort` always returns void in PHP; the array is mutated in-place.
///
/// # PHP Behavior
/// PHP's `ksort()` sorts an array by keys in ascending order, maintaining key-value correlations.
/// The return value is always `true` (1) in PHP, but since the return is typically ignored,
/// this emitter discards the return and always emits `PhpType::Void`.
///
/// # Side Effects
/// The runtime `__rt_ksort` helper mutates the array in-place. COW is handled by the caller
/// (via `emit_expr` on the array argument) before this function is invoked.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ksort()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- sort associative array by keys ascending --
    abi::emit_call_label(emitter, "__rt_ksort");                                // call the target-aware runtime helper that sorts associative-array keys ascending in place

    Some(PhpType::Void)
}
