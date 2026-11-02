//! Purpose:
//! Home of the PHP `uksort` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` derives the comparator parameter type from the array's KEY type — `uksort`
//!   compares array keys, not values — so an unannotated comparator over a string-keyed
//!   array types its parameters as `Str`. Returns `Void`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "uksort",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Uksort,
    ),
}

/// Validates the array and comparator callback arguments for a `uksort` call.
///
/// `uksort` compares array KEYS, so both comparator parameters are typed from the array's
/// key type: `Int` for an indexed array, the declared key type for an associative one. An
/// unannotated closure parameter inherits that type; explicit declarations stay
/// authoritative. Arity (exactly 2) is pre-validated by the registry.
/// Returns `Ok(PhpType::Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let key_ty = crate::types::checker::builtins::array_key_type(&arr_ty);
    let label = format!("{}() callback", cx.name);
    let callback_arg_types = [key_ty.clone(), key_ty];
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &callback_arg_types,
        cx.span,
        cx.env,
        &label,
    )?;
    Ok(PhpType::Bool)
}
