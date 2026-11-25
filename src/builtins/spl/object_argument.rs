//! Purpose:
//! Shared checker and effect rules for SPL builtins whose single PHP parameter is `object`.
//!
//! Called from:
//! - `spl_object_id` and `spl_object_hash` builtin homes.
//!
//! Key details:
//! - A statically known object keeps the throw-free contract.
//! - A boxed `mixed`, a union that can hold an object, and a callable (a Closure is an object)
//!   are checked at run time, so the call may throw PHP's argument `TypeError`.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinEffects, BuiltinSemanticInput, BuiltinSemantics, EffectsFn,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, RuntimeFnId};
use crate::types::PhpType;

/// Returns whether a value of this static type can reach an `object` parameter at run time.
pub(super) fn accepts_object_argument(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Mixed | PhpType::Callable | PhpType::DeclaredCallable => true,
        PhpType::Union(members) => members.iter().any(accepts_object_argument),
        _ => false,
    }
}

/// Validates the single `object` argument and returns the builtin's result type.
///
/// A type that can never hold an object stays a compile error, as before; anything that may
/// hold one is accepted and checked by the backend with PHP's exact `TypeError`.
pub(super) fn check_object_argument(
    cx: &mut BuiltinCheckCtx,
    name: &str,
    result: PhpType,
) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !accepts_object_argument(&ty) {
        return Err(CompileError::new(
            cx.span,
            &format!("{name}() argument must be an object"),
        ));
    }
    Ok(result)
}

/// Adds `MAY_THROW` to `base` unless the argument is statically an object.
pub(super) fn object_argument_effects(base: Effects, input: &BuiltinSemanticInput<'_>) -> Effects {
    if input
        .arg_types
        .iter()
        .all(|ty| matches!(ty.codegen_repr(), PhpType::Object(_)))
    {
        return base;
    }
    base | Effects::MAY_THROW
}

/// Builds the runtime-call semantics of an `object`-argument builtin with argument-aware effects.
pub(super) const fn object_argument_semantics(
    target: RuntimeFnId,
    effects: EffectsFn,
) -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(target);
    semantics.effects = BuiltinEffects::Shared(effects);
    semantics
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Objects, boxed values, callables and object-capable unions reach the runtime check.
    #[test]
    fn object_capable_types_are_accepted_and_scalars_are_not() {
        for ty in [
            PhpType::Object(String::new()),
            PhpType::Object("ArrayObject".into()),
            PhpType::Mixed,
            PhpType::Callable,
            PhpType::DeclaredCallable,
            PhpType::Union(vec![PhpType::Object("A".into()), PhpType::Void]),
            PhpType::Union(vec![PhpType::Int, PhpType::Mixed]),
        ] {
            assert!(accepts_object_argument(&ty), "{ty:?}");
        }
        for ty in [
            PhpType::Int,
            PhpType::Str,
            PhpType::Float,
            PhpType::Bool,
            PhpType::Array(Box::new(PhpType::Mixed)),
            PhpType::Union(vec![PhpType::Int, PhpType::Str]),
        ] {
            assert!(!accepts_object_argument(&ty), "{ty:?}");
        }
    }

    /// Only a statically known object keeps the throw-free effect contract.
    #[test]
    fn runtime_checked_arguments_may_throw() {
        let base = Effects::READS_HEAP;
        for (ty, throws) in [
            (PhpType::Object("A".into()), false),
            (PhpType::Mixed, true),
            (PhpType::Callable, true),
            (PhpType::Union(vec![PhpType::Object("A".into()), PhpType::Void]), true),
        ] {
            let arg_types = [ty.clone()];
            let input = BuiltinSemanticInput {
                name: "spl_object_id",
                args: &[],
                arg_types: &arg_types,
                span: crate::span::Span::dummy(),
            };
            let effects = object_argument_effects(base, &input);
            assert_eq!(effects.contains(Effects::MAY_THROW), throws, "{ty:?}");
            assert!(effects.contains(Effects::READS_HEAP), "{ty:?}");
        }
    }
}
