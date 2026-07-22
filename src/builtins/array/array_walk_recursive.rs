//! Purpose:
//! Home of the PHP `array_walk_recursive` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` validates the array and callback arguments using the contextual element type.
//!   Returns `Void`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_walk_recursive",
    area: Array,
    params: [ref array: Mixed, callback: Mixed],
    returns: Void,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayWalkRecursive,
    ),
    summary: "Applies a user function recursively to every member of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-walk-recursive.php",
}

/// Validates the array and callback arguments for an `array_walk_recursive` call.
///
/// Infers the array, derives its element type, and checks the callback signature contextually.
/// Arity (exactly 2) is pre-validated by the registry. Returns `Ok(PhpType::Void)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let callback_arg_types = [crate::types::checker::builtins::array_element_type(&arr_ty)];
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &callback_arg_types,
        cx.span,
        cx.env,
        &format!("{}() callback", cx.name),
    )?;
    Ok(PhpType::Void)
}
