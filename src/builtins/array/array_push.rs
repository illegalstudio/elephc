//! Purpose:
//! Home of the PHP `array_push` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(variadic(["array"], "values"))`: `array`
//!   by-ref plus a variadic `values` param. The legacy CHECK arm enforced exactly 2
//!   arguments, so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity`
//!   only; `function_sig` and the parity gate keep the variadic shape from the golden.
//! - The `ref` marker on `array` is mandatory — it is what makes by-reference mutation
//!   lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - Returns `Void` (not PHP's int count) — reproducing the legacy behavior exactly.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_push` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
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
    lower: lower,
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

/// Lowers an `array_push` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_push(ctx, inst)
}
