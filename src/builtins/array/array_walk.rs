//! Purpose:
//! Home of the PHP `array_walk` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` validates the array and callback arguments using the contextual element type.
//!   Boxed PHP arrays expose a writable Mixed value slot and a Mixed key slot. Returns `Void`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_walk",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayWalk,
    ),
}

/// Validates the array and callback arguments for an `array_walk` call.
///
/// Infers the array and checks the callback contextually against its element type, adding the
/// array's key type as a second parameter when the callback is known to accept it.
/// Arity (exactly 2) is pre-validated by the registry. Returns `Ok(PhpType::Void)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    crate::types::checker::builtins::check_array_walk_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &arr_ty,
        cx.span,
        cx.env,
        &format!("{}() callback", cx.name),
    )?;
    Ok(PhpType::Void)
}
