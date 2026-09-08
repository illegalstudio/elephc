//! Purpose:
//! Home of the PHP `array_diff_key` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` reproduces the legacy rule: the first argument must be an indexed or
//!   associative array, and the result preserves that first-operand type. A check hook
//!   is required because the return type depends on the inferred first-argument type.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_diff_key",
    check: check,
    semantics: array_diff_key_semantics(),
}

/// Builds semantics that preserve the first operand's concrete container representation.
const fn array_diff_key_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayDiffKey);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the first operand type because filtering keys preserves its storage shape.
fn eir_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    input
        .arg_types
        .first()
        .cloned()
        .unwrap_or(PhpType::Mixed)
}

/// Validates the first argument is an array and returns its (preserved) type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. The first argument is
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. The result preserves the first-operand array shape.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty1, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    Ok(ty1)
}
