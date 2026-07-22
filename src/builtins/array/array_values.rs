//! Purpose:
//! Home of the PHP `array_values` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy return-type rule: the result is an indexed
//!   `Array` whose element type is the input array's value type (the element type
//!   for an indexed array, the value type for an associative array). A check hook
//!   is required because the return type depends on the inferred argument type.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_values",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayValues,
    ),
    summary: "Returns all the values of an array, re-indexed numerically.",
    php_manual: "https://www.php.net/manual/en/function.array-values.php",
}

/// Returns the re-indexed value-array type for an `array_values` call.
///
/// The result is an indexed `Array` carrying the input array's value type. The
/// argument is re-inferred here to drive the return type; the registry already
/// inferred it once for side effects, and arity is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem_ty) => Ok(PhpType::Array(elem_ty)),
        PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
        _ => Err(CompileError::new(
            cx.span,
            "array_values() argument must be array",
        )),
    }
}
