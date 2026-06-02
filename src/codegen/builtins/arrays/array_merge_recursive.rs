//! Purpose:
//! Emits PHP `array_merge_recursive` builtin calls over two associative arrays.
//! Materializes both array pointers and delegates the recursive merge to the runtime helper.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Operates on hash inputs; the runtime recurses on array collisions and combines scalar collisions.
//! - Scalar indexed-array inputs are converted to integer-keyed hashes by the shared `hash_arg_call` helper.

use crate::codegen::builtins::arrays::hash_arg_call::emit_two_hash_arg_call;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_merge_recursive` builtin.
///
/// `array_merge_recursive($a, $b)` merges two associative arrays: integer-keyed entries
/// append with renumbering; string keys that collide recurse when both values are arrays,
/// otherwise the values are combined into a list. The result preserves the first array's
/// key space (widened where the second array contributes).
///
/// # Codegen
/// - Evaluates `args[0]` (first hash), spills it, evaluates `args[1]` (second hash).
/// - Materializes both pointers and calls `__rt_array_merge_recursive`.
///
/// # Returns
/// `Some(arr_ty)` — the first argument's array type.
///
/// # ABI
/// - AArch64: first hash in `x0`, second hash in `x1`; result hash in `x0`.
/// - x86_64: first hash in `rdi`, second hash in `rsi`; result hash in `rax`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_merge_recursive()");
    let (ty0, ty1) =
        emit_two_hash_arg_call(args, emitter, ctx, data, "__rt_array_merge_recursive", None);

    // Scalar collisions combine into lists, so the result value type is always Mixed. The key
    // widens to Mixed when the two inputs disagree (e.g. an indexed input mixed with string keys).
    Some(PhpType::AssocArray {
        key: Box::new(PhpType::widen(ty0.hash_key_type(), ty1.hash_key_type())),
        value: Box::new(PhpType::Mixed),
    })
}
