//! Purpose:
//! Emits PHP `natsort` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use crate::codegen_support::abi;
use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the `natsort()` builtin, which sorts an array in place using natural order.
/// Takes a single array argument, ensures it is COW-safe, stores the array pointer for mutation,
/// then calls `__rt_natsort` to perform the actual sort. Returns `PhpType::Void`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("natsort()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- sort array using natural order algorithm --
    abi::emit_call_label(emitter, "__rt_natsort");                              // call the target-aware runtime helper that sorts indexed integer arrays by natural ascending order

    Some(PhpType::Void)
}
