//! Purpose:
//! Home of the PHP `array_unique` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP preserves the KEY of every surviving element, so de-duplicating an indexed array
//!   yields a SPARSE result — `array_unique([1,2,2,3,1])` has keys `[0, 1, 3]` and no key 2.
//!   That shape is a hash, which is why an indexed input returns
//!   `AssocArray { key: Int, value: T }` rather than the input type echoed back.
//! - An associative input already records its keys and comes back unchanged.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "array_unique",
    area: Array,
    params: [array: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayUnique,
    ),
    summary: "Removes duplicate values from an array.",
    php_manual: "https://www.php.net/manual/en/function.array-unique.php",
}

/// Returns the key-preserving result type for an `array_unique` call.
///
/// Each survivor keeps its ORIGINAL key, so an indexed input yields a sparse result:
/// `[1,2,2,3,1]` keeps keys `0, 1, 3`. A dense indexed array cannot express that, so an
/// indexed input widens to `AssocArray { key: Int, value: T }` — the same shape
/// `array_reverse($a, true)` already returns for the same reason. An associative input keeps
/// its own type.
///
/// Non-array arguments are rejected. The argument is re-inferred here; the registry already
/// inferred it once for side effects, and arity is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(elem) => Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        }),
        PhpType::AssocArray { .. } => Ok(ty),
        _ => Err(CompileError::new(
            cx.span,
            "array_unique() argument must be array",
        )),
    }
}
