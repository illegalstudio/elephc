//! Purpose:
//! Home of the PHP `array_slice` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: a slice preserves the array shape, so the
//!   return type is the (array-or-assoc) input type unchanged; a boxed `Mixed`/`Union`
//!   input yields `Mixed`. A check hook is required because the return type depends on
//!   the inferred first-argument type.
//! - The declared signature carries the golden param list (`array`, `offset`,
//!   `length`), with `length` optional (default `null`), so the registry's
//!   `check_arity` accepts 2 or 3 arguments — matching the legacy CHECK arm.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_slice",
    area: Array,
    params: [array: Mixed, offset: Mixed, length: Mixed = DefaultSpec::Null],
    returns: Mixed,
    check: check,
    semantics: array_slice_semantics(),
    summary: "Extracts a slice of an array.",
    php_manual: "https://www.php.net/manual/en/function.array-slice.php",
}

/// Builds semantics with the boxed-Mixed indexed result layout used by the slice runtime.
const fn array_slice_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArraySlice);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the representation-safe indexed array type for typed and boxed source arrays.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Returns the slice's array type for an `array_slice` call.
///
/// A slice preserves the input array shape, so the (array-or-assoc) first-argument
/// type is returned unchanged; a boxed `Mixed`/`Union` first argument yields `Mixed`.
/// Non-array first arguments are rejected. The first argument is re-inferred here;
/// the registry already inferred every argument once for side effects, and arity
/// (2 or 3) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Ok(PhpType::Mixed);
    }
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_slice() first argument must be array",
        ));
    }
    Ok(ty)
}
