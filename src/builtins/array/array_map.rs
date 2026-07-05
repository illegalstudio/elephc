//! Purpose:
//! Home of the PHP `array_map` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["callback","array"], "arrays")` (two
//!   required params plus a variadic `arrays`). The legacy CHECK arm required exactly
//!   2 arguments, so `min_args: 2, max_args: 2` reproduce that enforcement in
//!   `check_arity` only; `function_sig` and the parity gate keep the variadic shape.
//! - `check` validates that the second argument is an indexed array and infers the
//!   callback return element type; the result preserves the input array element type
//!   unless the callback returns Mixed.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_map` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_map",
    area: Array,
    params: [callback: Mixed, array: Mixed],
    variadic: "arrays",
    min_args: 2,
    max_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Applies a callback to the elements of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-map.php",
}

/// Returns the mapped array type for an `array_map` call.
///
/// Validates that the second argument is an indexed array, checks the callback
/// with a dummy element argument, and derives the result element type from the
/// callback return type. Arity (exactly 2 args) is pre-validated by `check_arity`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    match arr_ty {
        PhpType::Array(elem_ty) => {
            let arr_ty = PhpType::Array(elem_ty.clone());
            let dummy_args = vec![
                crate::types::checker::builtins::dummy_arg_for_array_scalar_elem(
                    &arr_ty, cx.span,
                ),
            ];
            let callback_ret_ty =
                crate::types::checker::builtins::check_callback_builtin_call(
                    cx.checker,
                    &cx.args[0],
                    &dummy_args,
                    cx.span,
                    cx.env,
                    "array_map() callback",
                )?;
            let result_elem_ty = if callback_ret_ty == PhpType::Mixed {
                Box::new(PhpType::Mixed)
            } else {
                elem_ty
            };
            Ok(PhpType::Array(result_elem_ty))
        }
        _ => Err(CompileError::new(
            cx.span,
            "array_map() second argument must be array",
        )),
    }
}

/// Lowers an `array_map` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_map(ctx, inst)
}
