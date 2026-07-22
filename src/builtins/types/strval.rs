//! Purpose:
//! Registers PHP's `strval` conversion with backend-neutral EIR semantics.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::types`.
//!
//! Key details:
//! - Lowering reuses the general EIR string cast instead of defining a builtin-specific opcode.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Immediate, IrType, Op};

builtin! {
    name: "strval",
    area: Types,
    params: [value: Mixed],
    returns: Str,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::MayAliasArguments,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "runtime-selected strval requires a statically represented source value",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
    summary: "Gets the string value of a variable.",
    php_manual: "function.strval",
}

/// Returns the effect contract of the reusable EIR string cast.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::Cast.default_effects()
}

/// Lowers `strval` through the reusable EIR string-cast operation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::Cast,
        vec![call.operand(0)?],
        Some(Immediate::CastTarget(IrType::Str)),
        call.result_type.clone(),
        Op::Cast.default_effects(),
        Some(call.span),
    ))
}
