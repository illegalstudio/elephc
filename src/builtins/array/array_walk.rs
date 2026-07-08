//! Purpose:
//! Home of the PHP `array_walk` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` validates the array and callback arguments: infers the array element type,
//!   builds a dummy element argument for the callback, and validates the callback.
//!   Returns `Void`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_walk` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_walk",
    area: Array,
    params: [ref array: Mixed, callback: Mixed],
    returns: Void,
    check: check,
    lower: lower,
    summary: "Applies a user function to every member of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-walk.php",
}

/// Validates the array and callback arguments for an `array_walk` call.
///
/// Infers each argument, derives the element type from the array, builds a single
/// dummy element argument for callback validation, and checks the callback signature.
/// Arity (exactly 2) is pre-validated by the registry. Returns `Ok(PhpType::Void)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let dummy_args = vec![
        crate::types::checker::builtins::dummy_arg_for_array_scalar_elem(&arr_ty, cx.span),
    ];
    crate::types::checker::builtins::check_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &dummy_args,
        cx.span,
        cx.env,
        &format!("{}() callback", cx.name),
    )?;
    Ok(PhpType::Void)
}

/// Lowers an `array_walk` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_walk(ctx, inst)
}
