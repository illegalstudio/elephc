//! Purpose:
//! Home of the PHP `floatval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Checked user calls and prepared callable bodies reuse the rooted explicit float-cast graph.
//! - Metadata identifies the same conversion to original-source and typed checker validation.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Immediate, IrType, Op};
use crate::types::PhpType;

builtin! {
    contract: "floatval",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::ExplicitNumericCast(elephc_semantics::numeric_cast::NumericCastTarget::Float),
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Prepared(crate::builtins::semantics::callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the conservative effect contract of the reusable EIR float cast.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    let effects = Op::Cast.default_effects();
    if crate::semantics::conversions::checked_casts(input.semantic_profile, input.source_mode) { effects | crate::ir::Effects::MAY_THROW } else { effects }
}

/// Lowers `floatval` through the reusable EIR float-cast operation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let value = call.operand(0)?;
    if crate::semantics::conversions::checked_casts(call.semantic_profile, call.source_mode) {
        return ctx.emit_numeric_cast(elephc_semantics::numeric_cast::NumericCastTarget::Float,
            value, None, call.source_mode, call.span);
    }
    if matches!(ctx.value_php_type(value).codegen_repr(), PhpType::Object(_)) {
        return ctx.emit_numeric_cast(elephc_semantics::numeric_cast::NumericCastTarget::Float,
            value, None, call.source_mode, call.span);
    }
    if matches!(
        ctx.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) && ctx.has_class("GMP")
    {
        return Ok(ctx.emit_user_call(
            "__elephc_gmp_floatval_cast",
            vec![value],
            call.result_type.clone(),
            Some(call.span),
        ));
    }
    Ok(ctx.emit_value(
        Op::Cast,
        vec![value],
        Some(Immediate::CastTarget(IrType::F64)),
        call.result_type.clone(),
        Op::Cast.default_effects(),
        Some(call.span),
    ))
}
