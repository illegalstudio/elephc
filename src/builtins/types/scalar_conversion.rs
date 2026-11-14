//! Purpose:
//! Shares full scalar-conversion semantics between boolval and strval.
//!
//! Called from:
//! - Their semantic homes after the common argument planner evaluates the original value.
//!
//! Key details:
//! - Prepared callable bodies reuse the same target-aware cast graph as explicit syntax.
//! - PHP names never select an emitter and object hooks retain callback effects.

use crate::builtins::semantics::*;
use crate::parser::ast::CastType;

/// Builds a complete conversion descriptor with the selected result ownership contract.
pub(super) const fn semantics(target: CastType, ownership: BuiltinResultOwnership) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(crate::ir::Effects::php_conversion()),
        result_ownership: ownership,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Prepared(callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(match target {
            CastType::Bool => lower_bool, CastType::String => lower_string,
            _ => panic!("scalar conversion descriptor expects boolean or string"),
        }),
    }
}

/// Routes truthiness through the explicit cast's reviewed native object adapters.
fn lower_bool(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>)
    -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let value = call.operand(0)?;
    if !crate::semantics::conversions::checked_casts(call.semantic_profile, call.source_mode) {
        return Ok(ctx.emit_value(crate::ir::Op::IsTruthy, vec![value], None, call.result_type.clone(),
            crate::ir::Op::IsTruthy.default_effects(), Some(call.span)));
    }
    ctx.emit_scalar_cast(&CastType::Bool, value, call.source_mode, call.span)
}

/// Routes string conversion through the explicit cast's ownership and object-hook handling.
fn lower_string(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>)
    -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    ctx.emit_scalar_cast(&CastType::String, call.operand(0)?, call.source_mode, call.span)
}
