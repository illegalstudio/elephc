//! Purpose:
//! Home of the PHP `array_key_exists` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the second argument is an array and returns `Bool`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_key_exists",
    area: Array,
    params: [key: Mixed, array: Mixed],
    returns: Bool,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayKeyExists,
    ),
    summary: "Checks if the given key or index exists in the array.",
    php_manual: "https://www.php.net/manual/en/function.array-key-exists.php",
}

/// Validates that the second argument is an array and returns `Bool`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// This hook validates that `array` is an array and returns the `Bool` return type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_key_exists() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}
