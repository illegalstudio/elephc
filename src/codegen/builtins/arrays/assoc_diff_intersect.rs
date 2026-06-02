//! Purpose:
//! Emits PHP `array_diff_assoc` and `array_intersect_assoc` builtin calls over two associative arrays.
//! Materializes both array pointers and a mode selector, then delegates to the unified runtime helper.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Operates on hash inputs; the runtime compares keys and string-cast values.
//! - Scalar indexed-array inputs are converted to integer-keyed hashes by the shared `hash_arg_call` helper.

use crate::codegen::builtins::arrays::hash_arg_call::emit_two_hash_arg_call;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_diff_assoc` / `array_intersect_assoc` builtins.
///
/// Both compare entries of the first array against the second using **both** the key
/// and the string-cast value (`(string)$a === (string)$b`):
/// - `array_diff_assoc` keeps entries of `$array` whose (key, value) pair is absent from `$other`.
/// - `array_intersect_assoc` keeps entries whose (key, value) pair is present in `$other`.
///
/// # Codegen
/// - Evaluates `args[0]` (first hash), spills it, evaluates `args[1]` (second hash).
/// - Loads both pointers and a mode selector (0 = diff, 1 = intersect) and calls
///   `__rt_assoc_diff_intersect`, which iterates the first hash, looks each key up in the
///   second, and string-compares the values, retaining kept entries for the result.
///
/// # Returns
/// `Some(arr_ty)` — the first argument's array type (the result preserves its key space).
///
/// # ABI
/// - AArch64: hash1 in `x0`, hash2 in `x1`, mode in `x2`; result hash in `x0`.
/// - x86_64: hash1 in `rdi`, hash2 in `rsi`, mode in `rdx`; result hash in `rax`.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let intersect = name == "array_intersect_assoc";
    emitter.comment(if intersect {
        "array_intersect_assoc()"
    } else {
        "array_diff_assoc()"
    });
    let mode = if intersect { 1 } else { 0 };
    let (ty0, ty1) = emit_two_hash_arg_call(
        args,
        emitter,
        ctx,
        data,
        "__rt_assoc_diff_intersect",
        Some(mode),
    );

    // The runtime always produces a hash; key/value widen to Mixed when the two inputs disagree.
    Some(PhpType::two_input_hash_result(&ty0, &ty1))
}
