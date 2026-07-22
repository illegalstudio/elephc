//! Purpose:
//! Home of the PHP `array_column` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy rule: the first argument must be an `Array` of
//!   associative arrays; the result is an indexed `Array` of the associative value
//!   type. Other shapes are rejected. A check hook is required because the return type
//!   depends on the inferred argument type.
//! - Arity (exactly 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.
//!   Note elephc only supports the 2-argument form (`array`, `column_key`).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_column",
    area: Array,
    params: [array: Mixed, column_key: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayColumn,
    ),
    summary: "Returns the values from a single column of an array of arrays.",
    php_manual: "https://www.php.net/manual/en/function.array-column.php",
}

/// Returns the extracted-column array type for an `array_column` call.
///
/// The first argument must be an `Array` of associative arrays; the result is an
/// indexed `Array` of the associative value type. Other shapes are rejected. The
/// argument is re-inferred here to drive the return type; the registry already
/// inferred every argument once for side effects, and arity (exactly 2) is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
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
