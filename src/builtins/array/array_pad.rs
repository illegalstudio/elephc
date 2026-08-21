//! Purpose:
//! Home of the PHP `array_pad` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: padding preserves the array shape, so the
//!   return type is the (array-or-assoc) first-argument type unchanged. A check hook is
//!   required both to reject a non-array first argument and to echo its type back.
//! - Arity (exactly 3 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_pad",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayPad,
    ),
}

/// Returns the (shape-preserving) array type for an `array_pad` call.
///
/// Padding keeps the array shape, so the first-argument array/assoc type is returned
/// unchanged. A non-array first argument is rejected. The first argument is re-inferred
/// here; the registry already inferred every argument once for side effects, and arity
/// (exactly 3) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let ty = ty.array_or_false_member().cloned().unwrap_or(ty);
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_pad() first argument must be array",
        ));
    }
    Ok(ty)
}
