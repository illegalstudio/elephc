//! Purpose:
//! Home of the PHP `array_column` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Concrete associative rows preserve their value type in the indexed result.
//!   Declared PHP arrays use runtime row lookup and return indexed Mixed cells.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//!   Note elephc only supports the 2-argument form (`array`, `column_key`).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_column",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayColumn,
    ),
}

/// Returns the extracted-column array type for an `array_column` call.
///
/// Concrete associative rows determine the result element type; declared PHP
/// arrays require runtime row lookup and return Mixed elements. The
/// argument is re-inferred here to drive the return type; the registry already
/// inferred every argument once for side effects, and arity (exactly 2) is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if ty.is_php_array() {
        return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
    }
    match ty {
        PhpType::Array(inner) => match *inner {
            PhpType::AssocArray { value, .. } => Ok(PhpType::Array(value)),
            _ => Err(CompileError::new(
                cx.span,
                "array_column() requires an array of associative arrays",
            )),
        },
        _ => Err(CompileError::new(
            cx.span,
            "array_column() first argument must be array",
        )),
    }
}
