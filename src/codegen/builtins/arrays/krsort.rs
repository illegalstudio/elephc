//! Purpose:
//! Emits PHP `krsort` builtin calls that mutate array arguments in place.
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

/// Emits a call to the `krsort` runtime helper, which sorts an associative array
/// by its keys in descending order, mutating the array in place.
///
/// # Arguments
/// - `_name`: Unused (builtin dispatch is by arity/signature).
/// - `args`: Must contain exactly one argument — the array to sort.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context (carries variable layout, ownership state).
/// - `data`: Data section for relocations and constant data.
///
/// # Returns
/// Always returns `Some(PhpType::Void)` because `krsort` has no meaningful return value
/// in PHP — it operates purely by side effect (in-place mutation).
///
/// # Safety & Ownership
/// The single argument is emitted as a reference-like operand so the runtime helper
/// writes back to the caller's storage. No value-temp preevaluation occurs, preserving
/// PHP's semantics where the original variable is modified directly.
///
/// # PHP Semantics
/// `krsort($arr)` sorts `$arr` by keys in descending order. Flags (e.g., `SORT_REGULAR`)
/// are not yet supported; the runtime helper uses default comparison.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("krsort()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- sort associative array by keys descending --
    abi::emit_call_label(emitter, "__rt_krsort");                               // call the target-aware runtime helper that sorts associative-array keys descending in place

    Some(PhpType::Void)
}
