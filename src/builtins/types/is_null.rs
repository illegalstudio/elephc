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
    name: "is_null",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
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
        callable: BuiltinCallablePolicy::StaticOnly(
            "runtime-selected is_null requires a statically represented source value",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
    summary: "Checks whether a variable is null.",
    php_manual: "function.is-null",
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
