//! Purpose:
//! Home of the PHP `array_walk` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The signature is `array` (by reference), `callback`, and an optional `arg` handed to the
//!   callback as its third argument; it returns `true`. Lowering does not use the native runtime:
//!   `crate::ir_lower::expr::compat_preludes` redirects every call to the helpers built in
//!   `crate::array_walk_prelude`, which carry the measurements for why.
//! - `check` validates the array and callback arguments using the contextual element type.
//!   Boxed PHP arrays expose a writable Mixed value slot and a Mixed key slot. Returns `Bool`.

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
/// Arity (2 or 3) is pre-validated by the registry. Returns `Ok(PhpType::Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let extra_arg = match cx.args.get(2) {
        Some(arg) => Some(cx.checker.infer_type(arg, cx.env)?),
        None => None,
    };
    crate::types::checker::builtins::check_array_walk_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &arr_ty,
        extra_arg,
        false,
        cx.span,
        cx.env,
        &format!("{}() callback", cx.name),
    )?;
    // Both walks always answer `true` in php 8.
    Ok(PhpType::Bool)
}
