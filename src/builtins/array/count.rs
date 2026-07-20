//! Purpose:
//! Home of the PHP `count` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the argument type (Array, AssocArray, Mixed, Union-of-countable, or
//!   Countable Object) and returns `Int`. The Countable interface check delegates to
//!   `cx.checker.class_implements_interface`.
//! - `max_args: 1` reproduces the legacy checker's exactly-1 enforcement: `mode` has a
//!   default so `min` derives to 1; capping `max` at 1 yields the standard
//!   "count() takes exactly 1 argument" diagnostic. The 2-param golden is preserved for
//!   FCC and parity.
//! - All accepted representations lower through typed `runtime.count` so a typed array
//!   value carrying the runtime null-container sentinel still raises PHP's catchable TypeError.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::semantics::{
    runtime_fn_semantics, with_argument_lowering, BuiltinArgumentLowering, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::union_member_is_countable_array;
use crate::types::PhpType;

builtin! {
    name: "count",
    area: Array,
    params: [value: Mixed, mode: Int = DefaultSpec::Int(0)],
    max_args: 1,
    returns: Int,
    check: check,
    semantics: count_semantics(),
    summary: "Counts all elements in an array or Countable object.",
    php_manual: "https://www.php.net/manual/en/function.count.php",
}

/// Builds typed runtime semantics while retaining count's one-visible-argument lowering rule.
const fn count_semantics() -> BuiltinSemantics {
    with_argument_lowering(
        runtime_fn_semantics(crate::ir::RuntimeFnId::Count),
        BuiltinArgumentLowering::Count,
    )
}

/// Validates the argument type and returns `Int`.
///
/// Accepts Array, AssocArray, Mixed (heterogeneous arrays), a Union where every member
/// is countable, or an Object that implements the `Countable` interface. Arity
/// enforcement (exactly 1 argument) is handled by the registry's `check_arity` via
/// `max_args: 1`. Returns a `CompileError` for non-countable types or non-Countable objects.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match &ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => Ok(PhpType::Int),
        PhpType::Union(members) if members.iter().all(union_member_is_countable_array) => {
            Ok(PhpType::Int)
        }
        PhpType::Object(class_name) => {
            if cx.checker.class_implements_interface(class_name, "Countable") {
                Ok(PhpType::Int)
            } else {
                Err(CompileError::new(
                    cx.span,
                    "count() object argument must implement Countable",
                ))
            }
        }
        _ => Err(CompileError::new(
            cx.span,
            "count() argument must be array or Countable object",
        )),
    }
}
