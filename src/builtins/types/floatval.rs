//! Purpose:
//! Home of the PHP `floatval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Lowering reuses the general EIR float cast instead of defining a builtin-specific opcode.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Immediate, IrType, Op};
use crate::types::PhpType;

builtin! {
    name: "floatval",
    area: Types,
    params: [value: Mixed],
    returns: Float,
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
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts),
        lowering: BuiltinLowering::Eir(lower),
    },
    summary: "Returns the float value of a variable.",
    php_manual: "function.floatval",
}

/// Returns the conservative effect contract of the reusable EIR float cast.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::Cast.default_effects()
}

/// Preserves the concrete source representations accepted by runtime callable wrappers.
fn callable_accepts(source: Option<&PhpType>) -> bool {
    source.is_some_and(|source| {
        matches!(
            source.codegen_repr(),
            PhpType::Bool
                | PhpType::Float
                | PhpType::Int
                | PhpType::Never
                | PhpType::Str
                | PhpType::Void
        )
    })
}

/// Lowers `floatval` through the reusable EIR float-cast operation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::Cast,
        vec![call.operand(0)?],
        Some(Immediate::CastTarget(IrType::F64)),
        call.result_type.clone(),
        Op::Cast.default_effects(),
        Some(call.span),
    ))
}
