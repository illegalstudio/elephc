//! Purpose:
//! Emits PHP `asort` builtin calls that mutate array arguments in place.
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

/// Emits code for the PHP `asort` builtin, which sorts an array by values
/// in ascending order while maintaining key-to-value associations.
///
/// This function:
/// - Evaluates the array expression and prepares it for COW (copy-on-write)
/// - Handles the ref-like mutation semantics so the caller's storage is updated
/// - Calls the `__rt_asort` runtime helper
///
/// # Arguments
/// - `_name`: Unused, matching the builtin emitter signature
/// - `args`: Must contain exactly one array argument
/// - `emitter`: Assembly emitter for the current target
/// - `ctx`: Codegen context with variable layout and metadata
/// - `data`: Data section for constants/literals
///
/// # Returns
/// Always returns `Some(PhpType::Void)` since `asort` has no return value
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("asort()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- sort associative array by values, maintaining key association --
    abi::emit_call_label(emitter, "__rt_asort");                                // call the target-aware runtime helper that sorts array values ascending while preserving key association

    Some(PhpType::Void)
}
