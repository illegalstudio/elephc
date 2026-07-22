//! Purpose:
//! Home of the PHP `intval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Lowering reuses the general EIR integer cast instead of defining a builtin-specific opcode.
//! - Declared with exactly one parameter `value` (no `base` param) matching the legacy golden signature.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Immediate, IrType, Op};
use crate::types::PhpType;

builtin! {
    name: "intval",
    area: Types,
    params: [value: Mixed],
    returns: Int,
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
    summary: "Returns the integer value of a variable.",
    php_manual: "function.intval",
}

/// Returns the conservative effect contract of the reusable EIR integer cast.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::Cast.default_effects()
}

/// Preserves the source representations accepted by runtime callable wrappers.
fn callable_accepts(source: Option<&PhpType>) -> bool {
    source.is_none_or(|source| {
        matches!(
            source.codegen_repr(),
            PhpType::Bool
                | PhpType::Float
                | PhpType::Int
                | PhpType::Mixed
                | PhpType::Never
                | PhpType::Str
                | PhpType::Union(_)
                | PhpType::Void
        )
    })
}

/// Lowers `intval` through the reusable EIR integer-cast operation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::Cast,
        vec![call.operand(0)?],
        Some(Immediate::CastTarget(IrType::I64)),
        call.result_type.clone(),
        Op::Cast.default_effects(),
        Some(call.span),
    ))
}
