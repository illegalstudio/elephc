//! Purpose:
//! Home of the PHP `array_push` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(variadic(["array"], "values"))`: `array`
//!   by-ref plus a variadic `values` param. The legacy CHECK arm enforced exactly 2
//!   arguments, so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity`
//!   only; `function_sig` and the parity gate keep the variadic shape from the golden.
//! - The `ref` marker on `array` is mandatory — it is what makes by-reference mutation
//!   lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - Returns `Void` (not PHP's int count) — reproducing the legacy behavior exactly.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_push",
    area: Array,
    params: [ref array: Mixed],
    variadic: "values",
    min_args: 2,
    max_args: 2,
    returns: Void,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayPush,
    ),
    summary: "Pushes one or more elements onto the end of array.",
    php_manual: "https://www.php.net/manual/en/function.array-push.php",
}

/// Validates the first argument is an indexed array for an `array_push` call.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are inferred
/// to produce any side effects; the first must be an indexed array or the call is rejected.
/// Returns `Void` — matching the legacy checker behavior.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let _val_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if let PhpType::Array(_) = arr_ty {
        Ok(PhpType::Void)
    } else {
        Err(CompileError::new(cx.span, "array_push() first argument must be array"))
    }
}
