//! Purpose:
//! Home of the PHP `array_unique` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` types the result as php shapes it: de-duplication PRESERVES the source keys, so an
//!   indexed `array<T>` becomes `AssocArray { key: Int, value: T }` — dropping the middle of
//!   `["a","b","a","c"]` leaves keys `{0,1,3}`, which a dense indexed array cannot represent.
//!   php's default `SORT_STRING` path re-adds each first occurrence with
//!   `zend_hash_index_add_new(…, num_key, …)` (ext/standard/array.c), never
//!   `zend_hash_next_index_insert`. A source that is already associative keeps its own shape.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_unique",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayUnique,
    ),
}

/// Returns the key-preserving array type for an `array_unique` call.
///
/// De-duplication keeps each survivor's ORIGINAL key, so an indexed array becomes an `AssocArray`
/// keyed by `Int`; a source that is already associative keeps its own shape. Non-array arguments
/// are rejected. The argument is re-inferred here; the registry already inferred it once for side
/// effects, and arity is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let ty = ty.array_or_false_member().cloned().unwrap_or(ty);
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_unique() argument must be array",
        ));
    }
    Ok(key_preserving_set_op_result(ty))
}

/// Maps a value set operation's first-operand type onto the key-preserving result php returns.
///
/// The survivors keep their source keys, so an indexed `array<T>` answers
/// `AssocArray { key: Int, value: T }`; an already-associative source keeps its own shape.
pub(crate) fn key_preserving_set_op_result(ty: PhpType) -> PhpType {
    match ty {
        PhpType::Array(elem) => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        },
        other => other,
    }
}
