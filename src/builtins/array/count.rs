//! Purpose:
//! Home of the PHP `count` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the argument type (Array, AssocArray, Mixed, Union-of-countable, or
//!   Countable Object, fallible array results, or fallible SimpleXML wrappers) and returns
//!   `Int`. The Countable interface check delegates to `cx.checker.class_implements_interface`.
//! - `$mode` accepts `COUNT_NORMAL` (`0`) and `COUNT_RECURSIVE` (`1`); anything else raises
//!   PHP's catchable `ValueError`. The guard lives in the backend
//!   (`codegen::lower_inst::builtins::lower_count`) because `$mode` may be a runtime value.
//! - All accepted representations lower through typed `runtime.count` so a typed array
//!   value carrying the runtime null-container sentinel still raises PHP's catchable TypeError.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, with_argument_lowering, BuiltinArgumentLowering, BuiltinEffects,
    BuiltinSemanticInput, BuiltinSemantics,
};
use crate::errors::CompileError;
use crate::types::checker::builtins::arrays::union_member_is_countable_array;
use crate::types::PhpType;

builtin! {
    contract: "count",
    check: check,
    semantics: count_semantics(),
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
/// countable member (including fallible SimpleXML wrappers), or an Object that may
/// implement the `Countable` interface at runtime. Arity
/// enforcement (1 or 2 arguments) is handled by the registry's `check_arity`; `$mode`'s
/// value range is a runtime `ValueError`, not a compile-time error, exactly like PHP.
/// Returns a `CompileError` for non-countable scalar types.
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
        PhpType::Union(members)
            if members.iter().any(union_member_is_countable_array)
                || union_is_fallible_simplexml(cx.checker, members) =>
        {
            Ok(PhpType::Int)
        }
        // A non-Countable object is accepted here and REFUSED AT RUN TIME, the way PHP does it.
        // Refusing at compile time looked stricter for free, but it stopped programs PHP runs:
        // a `count($plain)` inside a function that is never called took the whole program down,
        // where PHP printed its output and never reached the line. The lowering raises PHP's own
        // `TypeError`, and the class is known statically here so the message needs no run-time
        // lookup.
        PhpType::Object(_) => Ok(PhpType::Int),
        _ => Err(CompileError::new(
            cx.span,
            "count() argument must be array or Countable object",
        )),
    }
}

/// Returns whether a union is one SimpleXML wrapper plus only its documented failure arms.
fn union_is_fallible_simplexml(
    checker: &crate::types::checker::Checker,
    members: &[PhpType],
) -> bool {
    let mut wrapper_class: Option<&str> = None;
    for member in members {
        match member {
            PhpType::Object(class_name) if is_simplexml_countable_class(checker, class_name) => {
                if wrapper_class
                    .is_some_and(|existing| !existing.eq_ignore_ascii_case(class_name))
                {
                    return false;
                }
                wrapper_class = Some(class_name);
            }
            PhpType::False | PhpType::Void | PhpType::Never => {}
            _ => return false,
        }
    }
    wrapper_class.is_some()
}

/// Returns whether a class uses SimpleXML's native `Countable` object handler.
fn is_simplexml_countable_class(
    checker: &crate::types::checker::Checker,
    class_name: &str,
) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    class_name.eq_ignore_ascii_case("SimpleXMLElement")
        || checker.is_subclass_of(class_name, "SimpleXMLElement")
}
