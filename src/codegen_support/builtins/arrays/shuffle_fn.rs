//! Purpose:
//! Emits PHP `shuffle` builtin calls that mutate array arguments in place.
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

/// Emits codegen for PHP `shuffle($array)` which mutates the array argument in place.
///
/// The emitted sequence is:
/// 1. Evaluate and emit the array expression (result in x1:x2)
/// 2. Call `ensure_unique_arg` to prepare the array for COW (copy-on-write) semantics
/// 3. Call `store_mutating_arg` to write the array pointer back to caller storage
///    (shuffle can reorder elements, so the array pointer itself may change)
/// 4. Call `__rt_shuffle` runtime helper which reorders elements in place
///
/// Returns `PhpType::Void` because PHP's shuffle() returns bool (ignored by the compiler).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("shuffle()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- call runtime to randomly reorder array elements in place --
    abi::emit_call_label(emitter, "__rt_shuffle");                              // call the target-aware runtime helper that shuffles indexed arrays in place

    Some(PhpType::Void)
}
