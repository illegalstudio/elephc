//! Purpose:
//! Home of the PHP `array_rand` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the argument is an array and returns `Int` (the randomly
//!   selected integer index). The declared `returns: Mixed` is the FCC type.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_rand",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayRand,
    ),
}

/// Validates that the argument is an array and returns `Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// The runtime always returns a single random integer index from the array.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let ty = ty.array_or_false_member().cloned().unwrap_or(ty);
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_rand() argument must be array",
        ));
    }
    Ok(PhpType::Int)
}
