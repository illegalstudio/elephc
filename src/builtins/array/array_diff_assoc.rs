//! Purpose:
//! Home of the PHP `array_diff_assoc` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` reproduces the legacy rule: both arguments must be associative arrays or
//!   indexed arrays of scalars, and the result is the two-input hash result type. A
//!   check hook is required because the return type depends on the inferred arguments.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_diff_assoc",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayDiffAssoc,
    ),
}

/// Returns whether the native hash runtime can compare an operand: an associative array or an
/// indexed array of scalars.
pub(super) fn native_assoc_set_operand(ty: &PhpType) -> bool {
    matches!(ty, PhpType::AssocArray { .. }) || ty.is_scalar_indexed_array()
}

/// Validates both arguments as arrays and returns the result type of a key-and-value set
/// operation (`array_diff_assoc()`, `array_intersect_assoc()`).
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are re-inferred
/// here to drive the return type; the registry already inferred every argument once for side
/// effects. Operands the hash runtime handles -- associative arrays and indexed arrays of
/// scalars -- widen key/value to `Mixed` when they disagree, via
/// `PhpType::two_input_hash_result`. Any other array-compatible operand (an indexed array of
/// strings or arrays, a `mixed` value, a union, a declared `array`) is only a well-formed pair
/// at run time, so the call answers a declared PHP array and lowers through
/// `crate::array_gradual_prelude`, whose helper raises php's `TypeError` for a non-array. A
/// statically non-array operand is rejected.
pub(super) fn check_set_operands(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty2 = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if native_assoc_set_operand(&ty1) && native_assoc_set_operand(&ty2) {
        return Ok(PhpType::two_input_hash_result(&ty1, &ty2));
    }
    let array_like = |ty: &PhpType| {
        crate::types::checker::builtins::array_arg_is_gradually_acceptable(ty) || ty.is_php_array()
    };
    if array_like(&ty1) && array_like(&ty2) {
        return Ok(PhpType::php_array());
    }
    Err(CompileError::new(
        cx.span,
        &format!(
            "{}() arguments must be associative arrays or indexed arrays of scalars",
            cx.name
        ),
    ))
}

fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_set_operands(cx)
}
