//! Purpose:
//! Home of the PHP `clamp` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - A shared result resolver is required because the return type depends on all three argument
//!   types: all-Str returns Str, all-Int returns Int, Int/Float mix returns Float,
//!   anything else returns Mixed.
//! - PHP 8.6's signature is `clamp(mixed $value, mixed $min, mixed $max): mixed`. The
//!   int/float/string families keep the typed runtime call; every other family goes through the
//!   prelude's `__elephc_clamp()` helper, which is php-src's own userland polyfill and therefore
//!   inherits PHP's cross-type comparison rules (booleans, nulls, arrays, and objects).

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinCallablePolicy, BuiltinEffects,
    BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError, BuiltinResultOwnership,
    BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

builtin! {
    contract: "clamp",
    semantics: clamp_semantics(),
}

/// Builds clamp semantics with one result resolver shared by checker and EIR lowering.
const fn clamp_semantics() -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Shared(result_type),
        effects: BuiltinEffects::Static(RuntimeFnId::Clamp.effects()),
        // The helper copies the selected operand into a local before returning it. The
        // runtime acquires a fresh reference to the same object/array/string, so callable
        // wrappers own an independent Mixed cell while `===` still sees the same object.
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: crate::builtins::semantics::BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: crate::builtins::semantics::BuiltinTargetSupport::All,
        runtime_functions: crate::builtins::semantics::BuiltinRuntimeFunctions::One(
            RuntimeFnId::Clamp,
        ),
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Returns the most precise result type for `clamp($value, $min, $max)`.
///
/// All-string operands return `Str`; all-int return `Int`; int/float mix returns
/// `Float`; any other combination returns `Mixed`.
fn result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    if input.arg_types.iter().all(|ty| *ty == PhpType::Str) {
        PhpType::Str
    } else if input.arg_types.iter().all(|ty| *ty == PhpType::Int) {
        PhpType::Int
    } else if input
        .arg_types
        .iter()
        .all(|ty| matches!(ty, PhpType::Int | PhpType::Float))
    {
        PhpType::Float
    } else {
        PhpType::Mixed
    }
}

/// Lowers `clamp` either to the typed runtime call or to the mixed prelude helper.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let operands = call.operands.to_vec();
    // The shared result resolver answers `Mixed` exactly when at least one operand is
    // boolean, null, array, object, or itself mixed. Only that family needs the PHP
    // helper; int/float/string unions keep the typed runtime call, whose result storage
    // is the same `Int`/`Float`/`Str` representation the resolver declared.
    if call.result_type.codegen_repr() == PhpType::Mixed {
        return Ok(ctx.emit_user_call(
            "__elephc_clamp",
            operands,
            call.result_type.clone(),
            Some(call.span),
        ));
    }
    Ok(ctx.emit_runtime_call(
        RuntimeCallTarget::Function(RuntimeFnId::Clamp),
        operands,
        call.result_type.clone(),
        RuntimeFnId::Clamp.effects(),
        Some(call.span),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mixed prelude helper is kept in sync with the registered contract name.
    #[test]
    fn clamp_mixed_lowering_targets_the_prelude_helper() {
        assert_eq!(
            crate::clamp_prelude::CLAMP_HELPER,
            "__elephc_clamp",
            "the EIR lowering and the prelude declaration must name the same function"
        );
    }
}
