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
//! - A PHP array declaration can carry either storage shape and produces boxed Mixed values.
//! - A `mixed` argument (a `json_decode()` result, an untyped value) or a union with an array
//!   member is accepted and yields `Array<Mixed>`: the backend's boxed path opens the box,
//!   dispatches on the runtime tag, and raises PHP's `TypeError` for a non-array payload.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::boxed_value_may_hold_array;
use crate::types::PhpType;

builtin! {
    contract: "array_values",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayValues,
    ),
}

/// Returns the re-indexed value-array type for an `array_values` call.
///
/// The result is an indexed `Array` carrying the input array's value type; a boxed receiver
/// that may hold an array (`mixed`, or a union with an array member) yields `Array<Mixed>`,
/// because its element layout is only known once the box is opened. The
/// argument is re-inferred here to drive the return type; the registry already
/// inferred it once for side effects, and arity is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if ty.is_php_array() || boxed_value_may_hold_array(&ty) {
        return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
    }
    match ty {
        PhpType::Array(elem_ty) => Ok(PhpType::Array(elem_ty)),
        PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
        _ => Err(CompileError::new(
            cx.span,
            "array_values() argument must be array",
        )),
    }
}
