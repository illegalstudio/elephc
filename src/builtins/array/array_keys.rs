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
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before
//!   the hook fires; the inline arity check from the legacy arm is not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_keys",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayKeys,
    ),
}

/// Returns the key-array type for an `array_keys` call.
///
/// An indexed array produces `Array<Int>`; an associative array produces
/// `Array<key>`; a `Mixed` value produces `Array<Mixed>` because its runtime key kind
/// (int for indexed storage, int-or-string for hash storage) is only known once the box is
/// opened. An `array|false` union — what `stat()`, `file()` and `scandir()` return — is accepted
/// through the member the family's unwrap will hand over, since php compiles that and throws only
/// when the value really is `false`. Every other argument type is rejected — `array_keys(42)` and
/// `array_keys("s")` remain compile errors. The argument is re-inferred here to drive the return type; the
/// registry already inferred it once for side effects, and arity is pre-validated by the
/// registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let inferred = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // `array_keys(stat($p))` is ordinary php: the union compiles, and only a runtime `false`
    // throws. The unwrap the family shares is driven by `ARRAY_OR_FALSE_ARG_SITES`, which now
    // names this slot; the key type comes from the array member, exactly as it would without the
    // union around it.
    let ty = match inferred.array_or_false_member() {
        Some(member) => member.clone(),
        None => inferred,
    };
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
