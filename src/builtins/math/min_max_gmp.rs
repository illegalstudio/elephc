//! Purpose:
//! Shares the GMP-aware `min()` / `max()` EIR lowering used by both builtin homes.
//!
//! Called from:
//! - `crate::builtins::math::{min,max}`.
//!
//! Key details:
//! - Calls that cannot see a GMP value keep the existing `RuntimeFnId::Min`/`Max`
//!   backend and argument planning, including the single-array form.
//! - Any operand that may hold GMP at runtime (statically GMP, boxed Mixed/Union
//!   storage, or an array of either) is reduced by the prelude helper, which encodes
//!   php-src's asymmetric tie rule and coerces the other operand like PHP's operator.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Builds the dual scalar/GMP descriptor for one min/max target.
pub(super) const fn semantics(target: RuntimeFnId) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Static(Effects::all()),
        // min/max return one of their inputs, including a GMP object.
        result_ownership: BuiltinResultOwnership::MayAliasArguments,
        requirements: BuiltinRequirements::Static(target.requirements()),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(target),
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::BareArrayValues {
            arg: 0,
            sole: true,
        },
        callable: BuiltinCallablePolicy::DynamicRuntime(target),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Returns whether the representation is statically known to be a GMP object.
fn is_static_gmp(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Object(name) if name.eq_ignore_ascii_case("GMP"))
}

/// Returns whether the representation is runtime-typed enough to hide a GMP object.
fn is_dynamic(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns whether an array representation is statically known to contain GMP values.
fn array_holds_static_gmp(ty: &PhpType) -> bool {
    match ty.codegen_repr() {
        PhpType::Array(element) => is_static_gmp(&element),
        PhpType::AssocArray { value, .. } => is_static_gmp(&value),
        _ => false,
    }
}

/// Returns whether an array representation may hide GMP values behind a runtime type.
fn array_may_hold_dynamic_gmp(ty: &PhpType) -> bool {
    match ty.codegen_repr() {
        PhpType::Array(element) => is_static_gmp(&element) || is_dynamic(&element),
        PhpType::AssocArray { value, .. } => is_static_gmp(&value) || is_dynamic(&value),
        _ => false,
    }
}

/// Returns whether a value is an array that must go through the GMP reducer.
fn array_needs_reducer(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
        && array_may_hold_dynamic_gmp(ty)
}

/// Builds the strict/max/tie control operands shared by both helper calls.
fn helper_flags(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    want_max: bool,
    last_min_tie: bool,
) -> (crate::ir::ValueId, crate::ir::ValueId, crate::ir::ValueId) {
    let strict = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(crate::source::current_strict_types())),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    let max = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(want_max)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    let last_min_tie = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(last_min_tie)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    (strict.value, max.value, last_min_tie.value)
}

/// Pushes one operand into the reducer array as an owned boxed cell.
fn push_operand(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    array: crate::ir::ValueId,
    operand: crate::ir::ValueId,
) {
    let representation = ctx.value_php_type(operand).codegen_repr();
    let copied = if matches!(representation, PhpType::Mixed | PhpType::Union(_)) {
        ctx.emit_value(
            Op::MixedClone,
            vec![operand],
            None,
            PhpType::Mixed,
            Op::MixedClone.default_effects(),
            Some(call.span),
        )
    } else {
        ctx.emit_value(
            Op::MixedBox,
            vec![operand],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            Some(call.span),
        )
    };
    ctx.emit_void(
        Op::ArrayPush,
        vec![array, copied.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(call.span),
    );
    ctx.emit_void(
        Op::Release,
        vec![copied.value],
        None,
        Op::Release.default_effects(),
        Some(call.span),
    );
}

/// Unboxes the reducer's `mixed` return when the checked result type is more specific.
fn finish_reducer_result(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    result: LoweredBuiltinValue,
) -> LoweredBuiltinValue {
    if matches!(call.result_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return result;
    }
    ctx.emit_value(
        Op::MixedUnbox,
        vec![result.value],
        None,
        call.result_type.clone(),
        Op::mixed_unbox_effects(&call.result_type),
        Some(call.span),
    )
}

/// Lowers scalar min/max to the existing runtime and GMP operands to the prelude reducer.
///
/// The reducer is used for the variadic form as soon as one operand may hold GMP, and for
/// the single-array form when the array's elements may hold GMP. It receives one temporary
/// operand array and the tie-policy flag, leaving the scalar fast path untouched.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let want_max = call.name == "max";
    let target = if want_max { RuntimeFnId::Max } else { RuntimeFnId::Min };
    let static_gmp = call.operands.iter().any(|operand| {
        let ty = ctx.value_php_type(*operand);
        is_static_gmp(&ty) || array_holds_static_gmp(&ty)
    });
    let dynamic_gmp = call.operands.iter().any(|operand| {
        let ty = ctx.value_php_type(*operand);
        is_dynamic(&ty) || array_may_hold_dynamic_gmp(&ty)
    });
    let uses_reducer = static_gmp || (dynamic_gmp && ctx.has_class("GMP"));
    let single_array = call.operands.len() == 1
        && array_needs_reducer(&ctx.value_php_type(call.operands[0]));
    if !uses_reducer || (call.operands.len() == 1 && !single_array) {
        return Ok(ctx.emit_runtime_call(
            RuntimeCallTarget::Function(target),
            call.operands.to_vec(),
            call.result_type.clone(),
            Effects::all(),
            Some(call.span),
        ));
    }
    let last_min_tie = !want_max && call.operands.len() == 2;
    let (strict, max, last_min_tie) = helper_flags(ctx, call, want_max, last_min_tie);
    if call.operands.len() == 1 {
        let array = call.operands[0];
        let result = ctx.emit_user_call(
            "__elephc_gmp_minmax_array",
            vec![strict, max, last_min_tie, array],
            PhpType::Mixed,
            Some(call.span),
        );
        return Ok(finish_reducer_result(ctx, call, result));
    }
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(call.operands.len() as u32)),
        PhpType::Array(Box::new(PhpType::Mixed)),
        Op::ArrayNew.default_effects(),
        Some(call.span),
    );
    for operand in call.operands {
        push_operand(ctx, call, array.value, *operand);
    }
    let result = ctx.emit_user_call(
        "__elephc_gmp_minmax_array",
        vec![strict, max, last_min_tie, array.value],
        PhpType::Mixed,
        Some(call.span),
    );
    ctx.emit_void(
        Op::Release,
        vec![array.value],
        None,
        Op::Release.default_effects(),
        Some(call.span),
    );
    Ok(finish_reducer_result(ctx, call, result))
}
