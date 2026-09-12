//! Purpose:
//! Home of the PHP `array_keys` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy return-type rule: an indexed array yields
//!   `Array<Int>` (positional keys) while an associative array yields `Array<key>`.
//!   A check hook is required because the return type depends on the inferred
//!   argument type, which the `builtin!` `returns:` field cannot express.
//! - A `Mixed` argument (an array read out of a `mixed`-typed value: a builtin/prelude return,
//!   `json_decode()`, an index read on a `mixed` container) is ACCEPTED and yields
//!   `Array<Mixed>`, because the runtime key kind is only known once the box is opened. This
//!   matches `count()`, which has always accepted `Mixed`. The backend unboxes and dispatches
//!   on the runtime tag, raising PHP's `TypeError` when the box does not hold an array.
//! - EIR resolves key storage from its actual operand after reference/call coercions.
//!   Boxed receivers and promotable Mixed-element arrays need boxed int-or-string keys.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_keys",
    check: check,
    semantics: array_keys_semantics(),
}

/// Reconciles key result storage with the lowered source instead of stale checker-only precision.
const fn array_keys_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayKeys);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Selects boxed keys for dynamic layouts and preserves concrete key storage when proven.
fn eir_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    let key = match input.arg_types.first().map(PhpType::codegen_repr) {
        Some(PhpType::Array(elem)) if elem.codegen_repr() != PhpType::Mixed => PhpType::Int,
        Some(PhpType::AssocArray { key, .. }) => *key,
        _ => PhpType::Mixed,
    };
    PhpType::Array(Box::new(key))
}

/// Returns the key-array type for an `array_keys` call.
///
/// An indexed array produces `Array<Int>`; an associative array produces
/// `Array<key>`; a `Mixed` value produces `Array<Mixed>` because its runtime key kind
/// (int for indexed storage, int-or-string for hash storage) is only known once the box is
/// opened. Every other argument type is rejected — `array_keys(42)` and `array_keys("s")`
/// remain compile errors. The argument is re-inferred here to drive the return type; the
/// registry already inferred it once for side effects, and arity is pre-validated by the
/// registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if ty.is_php_array() {
        return Ok(PhpType::Array(Box::new(PhpType::Mixed)));
    }
    match ty {
        PhpType::Array(_) => Ok(PhpType::Array(Box::new(PhpType::Int))),
        PhpType::AssocArray { key, .. } => Ok(PhpType::Array(key)),
        PhpType::Mixed => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
        _ => Err(CompileError::new(
            cx.span,
            "array_keys() argument must be array",
        )),
    }
}
