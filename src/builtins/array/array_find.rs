//! Purpose:
//! Home of the PHP `array_find` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array","callback"])` (exactly 2 required params).
//!   The legacy CHECK arm also required exactly 2 arguments; no arity override is needed.
//! - `check` validates the first argument is an indexed array and validates the predicate
//!   callback with its contextual element type. Returns `PhpType::Mixed` (the matching element
//!   or null).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_find",
    area: Array,
    params: [array: Mixed, callback: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayFind,
    ),
    summary: "Returns the first element satisfying a predicate callback, or null.",
    php_manual: "https://www.php.net/manual/en/function.array-find.php",
}

/// Validates the predicate callback for an `array_find` call and returns `PhpType::Mixed`.
///
/// The first argument must be an indexed array. The callback is validated with the array
/// element type as context. Arity (exactly 2 args) is
/// pre-validated by `check_arity`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_)) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    let callback_arg_types = [crate::types::checker::builtins::array_element_type(&arr_ty)];
    let label = format!("{}() callback", cx.name);
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &callback_arg_types,
        cx.span,
        cx.env,
        &label,
    )?;
    Ok(PhpType::Mixed)
}
