//! Purpose:
//! Emits PHP `arsort` builtin calls that mutate array arguments in place.
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

/// Emits code for the PHP `arsort` builtin, which sorts an associative array by values
/// in descending order while maintaining key-to-value associations.
///
/// Inputs:
/// - `args[0]`: the array expression to sort (mutated in place)
/// - `emitter`: target assembly emitter
/// - `ctx`: codegen context (carries variable layout, ownership state)
/// - `data`: data section for embedded literals
///
/// Behavior:
/// - Evaluates the array expression and captures its type.
/// - Prepares the array for mutation via COW (copy-on-write) if needed.
/// - Stores the array pointer back to the caller-side storage for ref-like semantics.
/// - Calls `__rt_arsort` to perform the sort in-place.
///
/// Returns `Some(PhpType::Void)` on success.
///
/// Note: `_name` is unused; the catalog resolves the builtin by canonical name.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("arsort()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- sort associative array by values descending, maintaining key association --
    abi::emit_call_label(emitter, "__rt_arsort");                               // call the target-aware runtime helper that sorts array values descending while preserving key association

    Some(PhpType::Void)
}
