//! Purpose:
//! Emits PHP `array_replace` builtin calls over two associative arrays.
//! Materializes both array pointers and delegates the right-wins key merge to the runtime helper.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Operates on hash inputs; the runtime clones the first array and overwrites/appends the second.
//! - Scalar indexed-array inputs are converted to integer-keyed hashes by the shared `hash_arg_call` helper.

use crate::codegen::builtins::arrays::hash_arg_call::emit_two_hash_arg_call;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_replace` builtin.
///
/// `array_replace($array, $replacements)` returns a new array: a copy of `$array`
/// in which every key present in `$replacements` is overwritten (in place, keeping
/// the original position) and every new key is appended. Later values win.
///
/// # Codegen
/// - Delegates argument evaluation to `hash_arg_call::emit_two_hash_arg_call`, which evaluates
///   both arguments in source order (converting a scalar indexed input to a hash) and calls
///   `__rt_array_replace`, which shallow-clones the first hash and inserts every entry of the
///   second through `__rt_hash_set` (right-wins), retaining heap/string values.
///
/// # Returns
/// `Some(arr_ty.as_hash())` — the result is always an integer-keyed hash, so an indexed input is
/// reported as an associative result.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let recursive = name == "array_replace_recursive";
    emitter.comment(if recursive {
        "array_replace_recursive()"
    } else {
        "array_replace()"
    });
    let runtime_label = if recursive {
        "__rt_array_replace_recursive"
    } else {
        "__rt_array_replace"
    };
    let (ty0, ty1) = emit_two_hash_arg_call(args, emitter, ctx, data, runtime_label, None);

    // The runtime always produces a hash; key/value widen to Mixed when the two inputs disagree.
    Some(PhpType::two_input_hash_result(&ty0, &ty1))
}
