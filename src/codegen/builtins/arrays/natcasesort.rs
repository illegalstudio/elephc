//! Purpose:
//! Emits PHP `natcasesort` builtin calls that mutate array arguments in place.
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

/// Emits a call to the `natcasesort` runtime helper, which sorts the input array
/// in place using case-insensitive natural order. The array argument is evaluated
/// once, made unique (COW), and then a pointer to the potentially-reallocated array
/// is written back to caller storage before the sort routine is invoked.
///
/// Arguments:
/// - `args[0]`: the array to sort (must be an indexed integer-array for the runtime helper)
/// - `emitter`: writes the call sequence
/// - `ctx`: provides variable layout and mutating-arg storage info
/// - `data`: data section for literals and runtime metadata
///
/// Returns: `Some(PhpType::Void)` — the function has no PHP-visible return value,
/// but the call sequence produces side effects on the array argument.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("natcasesort()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    // -- sort array using case-insensitive natural order algorithm --
    abi::emit_call_label(emitter, "__rt_natcasesort");                          // call the target-aware runtime helper that sorts indexed integer arrays by case-insensitive natural order

    Some(PhpType::Void)
}
