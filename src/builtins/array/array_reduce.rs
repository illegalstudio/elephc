//! Purpose:
//! Home of the PHP `array_reduce` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The initial value defaults to null and the carry may change PHP type at each step.
//! - Callback inference receives a dynamic carry before an unannotated closure is checked.
//! - The backend returns an independently owned Mixed result, including for an empty source.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_reduce",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayReduce,
    ),
}

/// Infers each operand and checks a callback whose carry is not restricted to the initial type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let callback_arg_types = [
        PhpType::Mixed,
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
    if let Some(initial) = cx.args.get(2) {
        cx.checker.infer_type(initial, cx.env)?;
    }
    Ok(PhpType::Mixed)
}
