//! Purpose:
//! Home of the PHP `pow` builtin: its declaration and GMP-aware EIR lowering.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Scalar calls keep the existing `RuntimeFnId::Pow` backend unchanged.
//! - A statically known `GMP` operand dispatches through GMP's `**` adapter, so
//!   `pow($gmp, 10)` and `$gmp ** 10` share coercion rules and diagnostics.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
    ResultTypeFn,
};
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

builtin! {
    contract: "pow",
    semantics: pow_semantics(),
}

/// Returns whether one operand is a statically known GMP object.
fn is_static_gmp(value: &PhpType) -> bool {
    matches!(value.codegen_repr(), PhpType::Object(name) if name.eq_ignore_ascii_case("GMP"))
}

/// Returns whether one operand is runtime-typed enough to hide a GMP object.
fn is_dynamic(value: &PhpType) -> bool {
    matches!(value.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Reports whether the call has a GMP operand and therefore returns GMP.
fn gmp_pow_result(input: &BuiltinSemanticInput<'_>) -> PhpType {
    if input.arg_types.iter().any(is_static_gmp) {
        PhpType::Object("GMP".to_string())
    } else if input.arg_types.iter().any(is_dynamic) {
        PhpType::Mixed
    } else {
        PhpType::Float
    }
}

/// Accepts the scalar contract and GMP's runtime-owned operand rules.
fn validate(_input: &BuiltinSemanticInput<'_>) -> Result<(), CompileError> {
    Ok(())
}

/// The complete semantic descriptor for the dual scalar/GMP lowering.
const fn pow_semantics() -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::Shared(validate),
        result_type: BuiltinResultType::Shared(gmp_pow_result as ResultTypeFn),
        effects: BuiltinEffects::Static(Effects::all()),
        // GMP results are fresh objects and the scalar branch returns a non-heap number.
        // A Mixed callable wrapper boxes the scalar branch into a fresh cell, so the result
        // is always owned by the caller rather than aliasing an argument.
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(RuntimeFnId::Pow),
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::PreserveUncoerced,
        callable: BuiltinCallablePolicy::Dynamic(
            crate::builtins::semantics::callable_accepts_any_source,
        ),
        lowering: BuiltinLowering::Eir(lower_pow),
    }
}

/// Lowers scalar `pow()` to its runtime backend and GMP operands to the `**` adapter.
fn lower_pow(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let has_static_gmp = call
        .operands
        .iter()
        .any(|operand| is_static_gmp(&ctx.value_php_type(*operand)));
    let has_dynamic_gmp = ctx.has_class("GMP")
        && call
            .operands
            .iter()
            .any(|operand| is_dynamic(&ctx.value_php_type(*operand)));
    if !has_static_gmp && !has_dynamic_gmp {
        return Ok(ctx.emit_runtime_call(
            RuntimeCallTarget::Function(RuntimeFnId::Pow),
            call.operands.to_vec(),
            PhpType::Float,
            Effects::all(),
            Some(call.span),
        ));
    }
    let strict = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(crate::source::current_strict_types())),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    let mut operands = vec![strict.value];
    let mut boxes = Vec::new();
    for operand in call.operands {
        let representation = ctx.value_php_type(*operand).codegen_repr();
        if matches!(representation, PhpType::Mixed | PhpType::Union(_)) {
            operands.push(*operand);
            continue;
        }
        let boxed = ctx.emit_value(
            Op::MixedBox,
            vec![*operand],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(call.span),
        );
        boxes.push(boxed.value);
        operands.push(boxed.value);
    }
    // The helper returns a boxed `mixed` cell: it may hold a GMP object or a scalar.
    // Unbox it to the checked result type before handing it to the caller.
    let result = ctx.emit_user_call(
        "__elephc_gmp_pow_operator",
        operands,
        PhpType::Mixed,
        Some(call.span),
    );
    let result = if matches!(
        call.result_type.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        result
    } else {
        ctx.emit_value(
            Op::MixedUnbox,
            vec![result.value],
            None,
            call.result_type.clone(),
            Op::mixed_unbox_effects(&call.result_type),
            Some(call.span),
        )
    };
    for boxed in boxes {
        ctx.emit_void(
            Op::Release,
            vec![boxed],
            None,
            Op::Release.default_effects(),
            Some(call.span),
        );
    }
    Ok(result)
}
