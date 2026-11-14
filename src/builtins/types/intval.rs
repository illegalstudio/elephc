//! Purpose:
//! Home of the PHP `intval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The one-argument form lowers to the general EIR integer cast instead of a
//!   builtin-specific opcode; only the two-argument form needs a runtime target.
//! - Checked user calls and prepared callable bodies share exact scalar/radix graphs.
//!   Their metadata also identifies casts during validation before source folding.
//! - The declared signature is PHP's own `intval(mixed $value, int $base = 10)`. PHP applies
//!   `$base` only when `$value` is a string and otherwise ignores it, which is why the
//!   two-argument form lowers to `RuntimeFnId::IntvalBase` rather than a string-only helper:
//!   the backend keeps the plain cast for non-string subjects.
//! - Reference PHP 8.4 raises nothing for an out-of-range `$base`; `strtol()` fails with
//!   `EINVAL` and `intval("42", 1)` is simply `0`, so no `ValueError` is emitted here.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Immediate, IrType, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

builtin! {
    contract: "intval",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::ExplicitNumericCast(elephc_semantics::numeric_cast::NumericCastTarget::Int),
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(RuntimeFnId::IntvalBase),
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Prepared(crate::builtins::semantics::callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the conservative effect contract of the reusable EIR integer cast.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    let effects = Op::Cast.default_effects();
    if crate::semantics::conversions::checked_casts(input.semantic_profile, input.source_mode) { effects | crate::ir::Effects::MAY_THROW } else { effects }
}

/// Lowers `intval` through the reusable EIR integer-cast operation, or through the
/// base-aware runtime target when PHP's `$base` argument is supplied.
///
/// The single-argument spelling keeps the plain cast so the common case stays a primitive.
/// With a `$base` the whole decision moves to `RuntimeFnId::IntvalBase`, because PHP only
/// honors the base for string subjects and the subject's runtime type is not always known
/// here (a `Mixed` cell may or may not hold a string).
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let value = call.operand(0)?;
    if crate::semantics::conversions::checked_casts(call.semantic_profile, call.source_mode) {
        return ctx.emit_numeric_cast(elephc_semantics::numeric_cast::NumericCastTarget::Int,
            value, call.operands.get(1).copied(), call.source_mode, call.span);
    }
    if matches!(ctx.value_php_type(value).codegen_repr(), PhpType::Object(_)) {
        return ctx.emit_numeric_cast(elephc_semantics::numeric_cast::NumericCastTarget::Int,
            value, None, call.source_mode, call.span);
    }
    let value_type = ctx.value_php_type(value);
    if matches!(value_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && ctx.has_class("GMP")
    {
        let base = if call.operands.len() >= 2 {
            call.operand(1)?
        } else {
            ctx.emit_value(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(10)),
                PhpType::Int,
                Op::ConstI64.default_effects(),
                Some(call.span),
            )
            .value
        };
        return Ok(ctx.emit_user_call(
            "__elephc_gmp_intval_cast",
            vec![value, base],
            call.result_type.clone(),
            Some(call.span),
        ));
    }
    if call.operands.len() >= 2 {
        return Ok(ctx.emit_runtime_call(
            RuntimeCallTarget::Function(RuntimeFnId::IntvalBase),
            vec![call.operand(0)?, call.operand(1)?],
            call.result_type.clone(),
            Op::Cast.default_effects(),
            Some(call.span),
        ));
    }
    Ok(ctx.emit_value(
        Op::Cast,
        vec![call.operand(0)?],
        Some(Immediate::CastTarget(IrType::I64)),
        call.result_type.clone(),
        Op::Cast.default_effects(),
        Some(call.span),
    ))
}
