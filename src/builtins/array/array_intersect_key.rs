//! Purpose:
//! Home of the PHP `array_intersect_key` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` accepts indexed and associative arrays. Indexed results use associative storage
//!   because retaining a subset of integer keys can leave holes that indexed storage cannot represent.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_intersect_key",
    check: check,
    semantics: array_intersect_key_semantics(),
}

/// Builds semantics whose result follows the normalized first hash operand.
const fn array_intersect_key_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayIntersectKey);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the normalized first operand type used by the key-set runtime.
fn eir_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    input
        .arg_types
        .first()
        .map(key_set_result_type)
        .unwrap_or(PhpType::Mixed)
}

/// Validates the first argument is an array and returns its (preserved) type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. The first argument is
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. Indexed inputs normalize to hole-preserving hash storage.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(
        ty1,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    Ok(key_set_result_type(&ty1))
}

/// Returns the storage type capable of preserving every key from the first operand.
fn key_set_result_type(ty: &PhpType) -> PhpType {
    match ty.codegen_repr() {
        PhpType::Array(value) => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value,
        },
        other => other,
    }
}
