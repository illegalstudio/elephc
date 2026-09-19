//! Purpose:
//! Home of the PHP `array_keys` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` reproduces the legacy return-type rule: an indexed array yields
//!   `Array<Int>` (positional keys) while an associative array yields `Array<key>` -- with two
//!   exceptions, both because the declared type does not describe the runtime key form: a
//!   STRING-keyed hash yields `Array<Mixed>` because a reindexing sort can leave it holding
//!   integer keys, and an `array<mixed>` receiver yields `Array<Mixed>` because it can be
//!   hash-backed at run time and hold string keys (issue #1072).
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
/// opened. Every other argument type is rejected — `array_keys(42)` and `array_keys("s")`
/// remain compile errors. The argument is re-inferred here to drive the return type; the
/// registry already inferred it once for side effects, and arity is pre-validated by the
/// registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        // An `array<mixed>` value can be HASH-backed at run time -- `lower_dynamic_mixed_array_keys`
        // exists precisely to branch on that at run time -- so its keys can be strings, and
        // `Array<Int>` has nowhere to put them. The backend refused the pair outright, which made
        // `array_keys([1, "b", 2.5])` a compile error on an ordinary heterogeneous literal.
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            Ok(PhpType::Array(Box::new(PhpType::Mixed)))
        }
        PhpType::Array(_) => Ok(PhpType::Array(Box::new(PhpType::Int))),
        // A STRING-keyed hash answers `Array<Mixed>`, not `Array<Str>`. `sort()`/`rsort()`
        // reindex a hash to `0..n-1`, and the receiver keeps its declared key type across the
        // by-reference call -- the checker pins a reference alias root there rather than
        // retyping it -- so the keys can be integers while the static type still says string.
        // `Array<Str>` has nowhere to put an integer key, and the materializer persisted the
        // int-key sentinel as a string length instead (issue #1072). An `Array<Int>` hash needs
        // no widening: reindexing an int-keyed hash still yields int keys.
        PhpType::AssocArray { key, .. } if matches!(*key, PhpType::Str) => {
            Ok(PhpType::Array(Box::new(PhpType::Mixed)))
        }
        PhpType::AssocArray { key, .. } => Ok(PhpType::Array(key)),
        PhpType::Mixed => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
        _ => Err(CompileError::new(
            cx.span,
            "array_keys() argument must be array",
        )),
    }
}
