//! Purpose:
//! Home of the PHP `array_unshift` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(variadic(["array"], "values"))`: `array`
//!   by-ref plus a variadic `values` param. PHP accepts `array_unshift($a)` (no values,
//!   returns the unchanged count) and any number of prepended values, so `min_args: 1`
//!   is the only `check_arity` override; the maximum stays unbounded.
//! - The `ref` marker on `array` is mandatory — it is what makes by-reference mutation
//!   lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - `values` is variadic, so PHP rejects it as a named argument
//!   (`array_unshift() does not accept unknown named parameters`); only `array` is nameable.
//! - Returns `Int` — the new number of elements in the array.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_unshift",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayUnshift,
    ),
}

/// Validates the first argument is an array for an `array_unshift` call.
///
/// Arity (at least 1 arg) is pre-validated by `check_arity`. Every argument is inferred so
/// the prepended values still produce their side effects; the first must be an indexed or
/// associative array, including a boxed PHP array declaration. Returns the new element count.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    for index in 1..cx.args.len() {
        cx.checker.infer_type(&cx.args[index], cx.env)?;
    }
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        && !arr_ty.is_php_array()
    {
        return Err(CompileError::new(
            cx.span,
            "array_unshift() first argument must be array",
        ));
    }
    Ok(PhpType::Int)
}
