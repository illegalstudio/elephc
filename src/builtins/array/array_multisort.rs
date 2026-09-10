//! Purpose:
//! Home of the PHP `array_multisort` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `fixed(["array1","array2"])` with `ref_params = [true, true]`:
//!   exactly 2 by-ref params. The `ref` markers are mandatory for in-place mutation.
//! - `check` accepts concrete indexed arrays and the non-null PHP array contract used by
//!   declared parameters. The backend validates that boxed values still use indexed storage.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_multisort",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayMultisort,
    ),
}

/// Validates argument types for an `array_multisort` call.
///
/// Requires both arguments be concrete indexed arrays or declared PHP arrays. Arity (exactly 2)
/// is pre-validated by the registry. Returns `Ok(PhpType::Bool)` on success.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty2 = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if (!matches!(&ty1, PhpType::Array(_)) && !ty1.is_php_array())
        || (!matches!(&ty2, PhpType::Array(_)) && !ty2.is_php_array())
    {
        return Err(CompileError::new(cx.span, "array_multisort() arguments must be indexed arrays"));
    }
    Ok(PhpType::Bool)
}
