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
//! - `$mode` accepts `COUNT_NORMAL` (`0`) and `COUNT_RECURSIVE` (`1`); anything else raises
//!   PHP's catchable `ValueError`. The guard lives in the backend
//!   (`codegen::lower_inst::builtins::lower_count`) because `$mode` may be a runtime value.
//! - All accepted representations lower through typed `runtime.count` so a typed array
//!   value carrying the runtime null-container sentinel still raises PHP's catchable TypeError.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::builtins::semantics::{
    runtime_fn_semantics, with_argument_lowering, BuiltinArgumentLowering, BuiltinEffects,
    BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::union_member_is_countable_array;
use crate::types::PhpType;

builtin! {
    name: "count",
    area: Array,
    params: [value: Mixed, mode: Int = DefaultSpec::Int(0)],
    returns: Int,
    check: check,
    semantics: count_semantics(),
    summary: "Counts all elements in an array or Countable object.",
    php_manual: "https://www.php.net/manual/en/function.count.php",
}

/// Builds typed runtime semantics while retaining count's one-visible-argument lowering rule.
const fn count_semantics() -> BuiltinSemantics {
    let mut semantics = with_argument_lowering(
        runtime_fn_semantics(crate::ir::RuntimeFnId::Count),
        BuiltinArgumentLowering::Count,
    );
    semantics.effects = BuiltinEffects::Shared(effects);
    semantics
}

/// Resolves count's intrinsic read/throw contract from the checked receiver representation.
///
/// `MAY_THROW` is unconditional: besides the null-container `TypeError`, every call can raise
/// the `ValueError` for a `$mode` outside `COUNT_NORMAL`/`COUNT_RECURSIVE`, so the call must
/// never be treated as a removable pure call.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    match input.arg_types.first().map(PhpType::codegen_repr) {
        Some(PhpType::Array(_) | PhpType::AssocArray { .. }) => {
            crate::ir::Effects::READS_HEAP | crate::ir::Effects::MAY_THROW
        }
        _ => crate::ir::RuntimeFnId::Count.effects() | crate::ir::Effects::MAY_THROW,
    }
}

/// Validates the argument type and returns `Int`.
///
/// Accepts Array, AssocArray, Mixed (heterogeneous arrays), a Union with at least one
/// countable member, or an Object that implements the `Countable` interface. Arity
/// enforcement (1 or 2 arguments) is handled by the registry's `check_arity`; `$mode`'s
/// value range is a runtime `ValueError`, not a compile-time error, exactly like PHP.
/// Returns a `CompileError` for non-countable types or non-Countable objects.
///
/// The union rule used to require EVERY member to be countable, which refused
/// `count(fgetcsv($h))` and `count(file($p))` — the ordinary shape for a builtin returning
/// `array|false`. That was not strictness for its own sake: it was standing in for the
/// missing runtime `TypeError`, and relaxing it before the guard existed would have spread
/// the silent zero instead of removing it (measured — `count($falseValue)` answered 0 where
/// PHP is fatal). The guard now raises, so a union whose non-countable arm is taken behaves
/// exactly like PHP. A union with NO countable member is still refused: that call cannot
/// succeed, and a compile error beats a certain run-time fatal.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match &ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => Ok(PhpType::Int),
        PhpType::Union(members) if members.iter().any(union_member_is_countable_array) => {
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
