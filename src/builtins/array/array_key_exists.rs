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
    contract: "array_key_exists",
    check: check,
    semantics: crate::builtins::semantics::with_array_key_input(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayKeyExists),
        crate::builtins::array_key_inputs::ArrayKeyInput::Value { argument: 0, receiver: 1, nullable: false },
    ),
}

/// Validates that the second argument can carry an array and returns `Bool`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 2 arguments).
/// Boxed `Mixed` and union values are accepted because guarded arrays retain their dynamic packed
/// versus associative representation; lowering dispatches their runtime tags to the correct probe.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(
        arr_ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            "array_key_exists() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}
