//! Purpose:
//! Home of the PHP `array_shift` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array"]))`: exactly 1 argument,
//!   the `array` param is by-reference. The `ref` marker is mandatory — it is what makes
//!   by-reference mutation lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - `check` reproduces the legacy rule: `Array(elem)` yields the element type,
//!   `AssocArray { value, .. }` yields the value type, any other type is an error.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_shift",
    area: Array,
    params: [ref array: Mixed],
    returns: Mixed,
    check: check,
    semantics: array_shift_semantics(),
    summary: "Shifts an element off the beginning of array.",
    php_manual: "https://www.php.net/manual/en/function.array-shift.php",
}

/// Builds semantics whose EIR result preserves the builtin's nullable boxed payload.
const fn array_shift_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayShift);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns Mixed because an empty array produces null regardless of its element type.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Mixed
}

/// Returns the element type for an `array_shift` call.
///
/// The `array` argument is re-inferred to drive the return type. Arity (exactly 1) is
/// pre-validated by the registry. `Array(elem)` yields the element type; `AssocArray`
/// yields the value type; any other type is a compile error.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem) => Ok(*elem),
        PhpType::AssocArray { value, .. } => Ok(*value),
        _ => Err(CompileError::new(cx.span, "array_shift() argument must be array")),
    }
}
