//! Purpose:
//! Home of the PHP `array_reduce` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The shared PHP signature accepts two arguments and defaults the initial value to null.
//! - The current integer-carry AOT helper still requires an explicit initial value;
//!   reject its unsupported omitted-initial path instead of indexing a missing argument.
//! - `check` validates the callback with the inferred initial and array-element types.
//!   The return type is `PhpType::Int`, matching the legacy arm.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_reduce",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayReduce,
    ),
}

/// Validates the callback for an `array_reduce` call and returns `PhpType::Int`.
///
/// Uses the initial-value and array-element types as the two callback parameter contexts.
/// The PHP arity is checked by the registry; the legacy AOT carry limitation is checked here.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let Some(initial) = cx.args.get(2) else {
        return Err(CompileError::new(cx.span,
            "array_reduce() without an explicit initial value is not yet supported by the AOT backend"));
    };
    let initial_ty = cx.checker.infer_type(initial, cx.env)?;
    let callback_arg_types = [
        initial_ty,
        crate::types::checker::builtins::array_element_type(&arr_ty),
    ];
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &callback_arg_types,
        cx.span,
        cx.env,
        "array_reduce() callback",
    )?;
    Ok(PhpType::Int)
}
