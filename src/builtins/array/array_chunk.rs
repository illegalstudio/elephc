//! Purpose:
//! Home of the PHP `array_chunk` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: chunking an indexed `Array<elem>` yields a
//!   nested `Array<Array<elem>>`. Associative inputs are rejected (the lowering only
//!   supports indexed arrays), and non-array inputs are rejected too. A check hook is
//!   required because the return type depends on the inferred argument type.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_chunk",
    area: Array,
    params: [array: Mixed, length: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayChunk,
    ),
    summary: "Splits an array into chunks of the given size.",
    php_manual: "https://www.php.net/manual/en/function.array-chunk.php",
}

/// Returns the nested chunk-array type for an `array_chunk` call.
///
/// An indexed `Array<elem>` chunks into `Array<Array<elem>>`. Associative arrays are
/// rejected (only indexed arrays are supported), and non-array arguments are rejected.
/// The argument is re-inferred here to drive the return type; the registry already
/// inferred every argument once for side effects, and arity (exactly 2) is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem_ty) => Ok(PhpType::Array(Box::new(PhpType::Array(elem_ty)))),
        PhpType::AssocArray { .. } => Err(CompileError::new(
            cx.span,
            "array_chunk() argument must be indexed array",
        )),
        _ => Err(CompileError::new(
            cx.span,
            "array_chunk() first argument must be array",
        )),
    }
}
