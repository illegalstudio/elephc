//! Purpose:
//! Home of the PHP `array_search` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the second argument is an array and returns a union of the
//!   key type and Bool (false on not-found), or Int|Bool for indexed arrays.
//! - The full PHP signature is `array_search(mixed $needle, array $haystack, bool $strict = false)`
//!   (min=2, max=3) and is enforced verbatim: no `max_args` override narrows it. `strict`
//!   works positionally and as a named argument, and is honoured by `lower_array_search`.
//! - `strict` does not change the result type: PHP still returns the found key or `false`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_search",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArraySearch,
    ),
}

/// Validates haystack is an array and returns the key-or-false union type.
///
/// The registry's `check_arity` handles arity enforcement (2 or 3 arguments) and infers every
/// argument, including the optional `strict` flag, before this hook runs. For assoc arrays the
/// return is `key_type | bool`; for indexed arrays it is `int | bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let arr_ty = arr_ty.array_or_false_member().cloned().unwrap_or(arr_ty);
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_search() second argument must be array",
        ));
    }
    match arr_ty {
        PhpType::AssocArray { key, .. } => {
            Ok(cx.checker.normalize_union_type(vec![*key, PhpType::False]))
        }
        _ => Ok(PhpType::Union(vec![PhpType::Int, PhpType::False])),
    }
}
