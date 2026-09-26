//! Purpose:
//! Home of the PHP `array_push` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(variadic(["array"], "values"))`: `array`
//!   by-ref plus a variadic `values` param, so `min_args: 1` is the only `check_arity`
//!   override and the maximum is unbounded. It used to be pinned to `2, 2`, which rejected
//!   ordinary PHP (`array_push($a, 3, 4)`) with an arity error (issue #677).
//! - The `ref` marker on `array` is mandatory — it is what makes by-reference mutation
//!   lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - Returns `Int` — the new number of elements, as PHP does and as the `array_unshift`
//!   sibling already did. It used to return `Void`, so `$n = array_push($a, 1)` read `NULL`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_push",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayPush,
    ),
}

/// Validates indexed arrays and boxed PHP array declarations for an `array_push` call.
///
/// Arity (at least 1 arg) is pre-validated by `check_arity`. Every argument is inferred so the
/// appended values still produce their side effects; the first must be an indexed array or a
/// declared PHP array (boxed packed-or-hash storage) or the call is rejected. Returns `Int` — the
/// new element count.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    for index in 1..cx.args.len() {
        cx.checker.infer_type(&cx.args[index], cx.env)?;
    }
    if matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) || arr_ty.is_php_array() {
        Ok(PhpType::Int)
    } else {
        Err(CompileError::new(cx.span, "array_push() first argument must be array"))
    }
}
