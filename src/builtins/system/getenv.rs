//! Purpose:
//! Home of the PHP `getenv` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` to reflect PHP's behaviour where `getenv`
//!   returns the value string on success or `false` if the variable is unset.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "getenv",
    area: System,
    params: [name: Str],
    returns: Mixed,
    check: check,
    semantics: getenv_semantics(),
    summary: "Gets the value of an environment variable.",
}

/// Builds semantics whose EIR result matches the backend's string representation.
const fn getenv_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::Getenv);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the concrete EIR string layout produced for present and missing variables.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Str
}

/// Returns `Union(Str, Bool)` reflecting that `getenv` can return a string or `false`.
///
/// Infers the argument type to trigger type-environment side effects before returning
/// the normalized union type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::False]))
}
