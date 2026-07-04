//! Purpose:
//! Home of the PHP `array_unshift` builtin: its declaration, type-check hook, and lowering.
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
//! - Returns `Int` — the new number of elements in the array.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_unshift` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_unshift",
    area: Array,
    params: [ref array: Mixed],
    variadic: "values",
    min_args: 2,
    max_args: 2,
    returns: Int,
    check: check,
    lower: lower,
    summary: "Prepends one or more elements to the beginning of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-unshift.php",
}

/// Validates the first argument is an array for an `array_unshift` call.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are inferred
/// to produce any side effects; the first must be an indexed or associative array or the
/// call is rejected. Returns `Int` — the new element count.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_unshift() first argument must be array",
        ));
    }
    Ok(PhpType::Int)
}

/// Lowers an `array_unshift` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_unshift(ctx, inst)
}
