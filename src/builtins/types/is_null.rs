//! Purpose:
//! Home of the PHP `is_null` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - Lowering emits the reusable `IsNull` EIR predicate for every checked storage representation.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Op;

builtin! {
    contract: "is_null",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        // Reachable by run-time name like the other type predicates (`sprintf('is_%s', $type)`
        // in symfony/dependency-injection's `CheckTypeDeclarationsPass`): the wrapper passes the
        // value boxed, and `IsNull` answers for a boxed value as for any other.
        callable: BuiltinCallablePolicy::Dynamic(
            crate::builtins::semantics::callable_accepts_any_source,
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the conservative effect contract of the reusable EIR null predicate.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::IsNull.default_effects()
}

/// Lowers `is_null` to the reusable EIR null predicate.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::IsNull,
        vec![call.operand(0)?],
        None,
        call.result_type.clone(),
        Op::IsNull.default_effects(),
        Some(call.span),
    ))
}
