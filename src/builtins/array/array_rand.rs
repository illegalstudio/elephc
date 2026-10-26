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

/// Validates the array argument and returns PHP's `int|string|array` result union.
///
/// Direct AOT calls are rewritten onto `__elephc_array_rand`, which implements the
/// multi-key form; this builtin remains the first-class-callable/eval entry point.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_rand() argument must be array",
        ));
    }
    if let Some(num) = cx.args.get(1) {
        cx.checker.infer_type(num, cx.env)?;
    }
    Ok(PhpType::Union(vec![
        PhpType::Int,
        PhpType::Str,
        PhpType::Array(Box::new(PhpType::Mixed)),
    ]))
}
