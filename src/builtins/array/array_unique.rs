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
//! - Arity (1 or 2 arguments) is validated by the registry's `check_arity` before
//!   the hook fires. The optional `$flags` defaults to `SORT_STRING`, which the native runtime
//!   implements; any other comparison goes through `crate::array_gradual_prelude`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_unique",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayUnique,
    ),
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
    // `$flags` selects the comparison. The native runtime compares as strings, which is
    // `SORT_STRING` -- php's default -- so an omitted or literal `2` flag keeps it; any other
    // comparison (Laravel passes `SORT_REGULAR`, or a run-time value) lowers through
    // `crate::array_gradual_prelude` and answers a declared PHP array.
    if cx.args.len() > 1 && !crate::array_gradual_prelude::flags_are_sort_string(&cx.args[1]) {
        let array_like = crate::types::checker::builtins::array_arg_is_gradually_acceptable(&ty)
            || ty.is_php_array();
        if !array_like {
            return Err(CompileError::new(
                cx.span,
                "array_unique() argument must be array",
            ));
        }
        return Ok(PhpType::php_array());
    }
    match ty {
        PhpType::Array(elem) => Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        }),
        // Gradual inputs keep their own type: a `mixed`/union receiver only becomes an array at
        // runtime, so there is no element type to re-key here.
        PhpType::AssocArray { .. } | PhpType::Mixed => Ok(ty),
        // A null member is a TypeError at run time, never a result: `array_unique($this->p)`
        // over a `?array` property returns an array.
        PhpType::Union(members) => {
            let mut kept: Vec<PhpType> = members
                .iter()
                .filter(|member| !matches!(member, PhpType::Void))
                .cloned()
                .collect();
            Ok(match kept.len() {
                0 => PhpType::Union(members),
                1 => match kept.remove(0) {
                    PhpType::Array(elem) => PhpType::AssocArray {
                        key: Box::new(PhpType::Int),
                        value: elem,
                    },
                    other => other,
                },
                _ => PhpType::Union(kept),
            })
        }
        _ => Err(CompileError::new(
            cx.span,
            "array_unique() argument must be array",
        )),
    }
}
