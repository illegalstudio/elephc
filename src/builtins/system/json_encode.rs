//! Purpose:
//! Home of the PHP `json_encode` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates known flag/depth argument types while allowing gradual
//!   values to cross the integer boundary through the shared runtime coercion path.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "json_encode",
    check: check,
    semantics: json_encode_semantics(),
}

/// Builds semantics whose EIR result matches the runtime's boxed string-or-false value.
const fn json_encode_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::JsonEncode);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the representation-safe EIR type for the runtime-selected string-or-false result.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Mixed
}

/// Validates known flag and depth argument types and defers gradual values to runtime coercion.
///
/// Reports type errors at the span of the offending argument, not the call span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    for extra in &cx.args[1..] {
        let ty = cx.checker.infer_type(extra, cx.env)?;
        if !matches!(ty, PhpType::Int | PhpType::Mixed) {
            return Err(CompileError::new(
                extra.span,
                "json_encode() flags and depth must be integers",
            ));
        }
    }
    Ok(PhpType::Str)
}
