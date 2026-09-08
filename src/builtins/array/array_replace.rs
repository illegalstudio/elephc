//! Purpose:
//! Home of the PHP `array_replace` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array", "replacements"])` (two required
//!   params, no variadic), matching the registry signature. The
//!   param-derived bounds already require exactly 2 arguments, so no `min_args`/
//!   `max_args` override is needed; `check_arity` owns the arity contract.
//! - `check` enforces that both arguments are associative arrays or
//!   indexed arrays of scalars, and the result is the two-input hash result type. A
//!   check hook is required because the return type depends on the inferred arguments.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_replace",
    check: check,
    semantics: array_replace_semantics(),
}

/// Builds semantics that derive the concrete result hash from both operand types.
const fn array_replace_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayReplace);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the widened associative result shape used by the two-hash runtime helper.
fn eir_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    let Some(first) = input.arg_types.first() else {
        return PhpType::Mixed;
    };
    let Some(second) = input.arg_types.get(1) else {
        return first.clone();
    };
    PhpType::two_input_hash_result(first, second)
}

/// Validates both arguments are hash-compatible arrays and returns the merged hash type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. Both arguments are
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. Each operand must be an associative array or an
/// indexed array of scalars; the result widens key/value to `Mixed` when the operands
/// disagree, via `PhpType::two_input_hash_result`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty2 = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let accepted =
        |t: &PhpType| matches!(t, PhpType::AssocArray { .. }) || t.is_scalar_indexed_array();
    if !accepted(&ty1) || !accepted(&ty2) {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "{}() arguments must be associative arrays or indexed arrays of scalars",
                cx.name
            ),
        ));
    }
    Ok(PhpType::two_input_hash_result(&ty1, &ty2))
}
