//! Purpose:
//! Home of the PHP `array_merge_recursive` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&[], "arrays")`: any number of arrays, as in php.
//! - `check` validates that every argument is an array (including gradual array boundaries) and
//!   returns an `AssocArray` whose value type is always `Mixed` (scalar collisions
//!   combine into lists). The key type widens to `Mixed` when the two input key types
//!   disagree.
//! - Arity is pre-validated by `check_arity`; any count of arrays is valid, as in php.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_merge_recursive",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayMergeRecursive,
    ),
}

/// Validates every argument is a compatible array and returns the recursively-merged type.
///
/// php takes any number of arrays (`array_merge_recursive()` is `[]`). The native runtime merges
/// two operands of concrete layout; every other call (another count, or a `mixed`, union or
/// declared-`array` operand) answers a declared PHP array and lowers through
/// `crate::array_gradual_prelude`, whose helper raises php's `TypeError` for a non-array.
/// The native result is an `AssocArray` whose value type is always `Mixed` (scalar collisions
/// combine into lists); its key type widens to `Mixed` when the input key types disagree.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let accepted = crate::types::checker::builtins::array_arg_is_gradually_acceptable;
    let mut key: Option<PhpType> = None;
    let mut concrete = cx.args.len() == 2;
    for arg in cx.args.iter() {
        let ty = cx.checker.infer_type(arg, cx.env)?;
        if !accepted(&ty) {
            return Err(CompileError::new(
                cx.span,
                "array_merge_recursive() arguments must be arrays",
            ));
        }
        concrete &= matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. });
        key = Some(match key {
            None => ty.hash_key_type(),
            Some(previous) => PhpType::widen(previous, ty.hash_key_type()),
        });
    }
    if !concrete {
        return Ok(PhpType::php_array());
    }
    Ok(PhpType::AssocArray {
        key: Box::new(key.unwrap_or(PhpType::Int)),
        value: Box::new(PhpType::Mixed),
    })
}
