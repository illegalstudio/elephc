//! Purpose:
//! Shares the `min()` / `max()` result typing and EIR lowering used by both builtin homes.
//!
//! Called from:
//! - `crate::builtins::math::{min,max}`.
//!
//! Key details:
//! - A variadic call whose arguments are all statically `int`, or all statically `float`,
//!   keeps the scalar `RuntimeFnId::Min`/`Max` backend, as does the single-array form.
//! - Every other comparable variadic call (an `int` beside a `float`, a bool, null, a
//!   nullable or runtime-typed value, a checked arithmetic result that may have promoted to
//!   float, a string) is typed `mixed` and lowers to one `Op::MixedMinMax`
//!   selection, so the winner keeps its own value and runtime type as in PHP.
//! - A call that may meet a GMP object (a statically GMP operand, an array of them, or a
//!   boxed operand in a program that declares GMP) is reduced by the prelude helper
//!   instead. The helper encodes php-src's tie and `NAN` rules for the single-array, the
//!   two-argument and the longer variadic shape, and its PHP frame unwinds cleanly when the
//!   GMP comparator throws.
//! - A variadic call with an operand the runtime does not order like PHP (an array, a
//!   non-GMP object) keeps the scalar typing, whose backend rejects it at compile time.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, MinMaxRule, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Returns the checked result type of `min()` / `max()`.
///
/// The single-array form returns the array's element type. The variadic form returns one of
/// its arguments unchanged, so its static type is `int` only when every argument is an `int`
/// and `float` only when every argument is a `float`. Any other comparable mix (an `int`
/// beside a `float`, a bool, null, a nullable or runtime-typed value, a string, a GMP
/// object) is `mixed`: which argument wins, and therefore the result's type, is only known
/// at run time.
pub(super) fn check(cx: &mut BuiltinCheckCtx, name: &str) -> Result<PhpType, CompileError> {
    if cx.args.len() == 1 {
        return super::min_max_array_element_type(cx, name);
    }
    let mut types = Vec::with_capacity(cx.args.len());
    for arg in cx.args {
        types.push(cx.checker.infer_type(arg, cx.env)?);
    }
    Ok(variadic_result_type(&types))
}

/// Returns the static result type of a variadic call from its argument types.
fn variadic_result_type(types: &[PhpType]) -> PhpType {
    if types.iter().all(|ty| *ty == PhpType::Int) {
        return PhpType::Int;
    }
    if types.iter().all(|ty| *ty == PhpType::Float) {
        return PhpType::Float;
    }
    if types.iter().any(is_static_gmp) || types.iter().all(selects_at_runtime) {
        return PhpType::Mixed;
    }
    // An array, object or other operand the runtime selection does not order like PHP keeps
    // the scalar typing, whose backend rejects that operand with a compile error.
    if types.iter().any(|ty| *ty == PhpType::Float) { PhpType::Float } else { PhpType::Int }
}

/// Returns whether the runtime selection compares a value of this type like PHP does.
///
/// These are the scalar shapes `__rt_php_compare` orders exactly: numbers, bools, null,
/// strings, and boxed values, which carry their own runtime tag.
fn selects_at_runtime(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Str
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::TaggedScalar
    )
}

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

/// Builds the strict/max/mode control operands shared by both helper calls.
///
/// `mode` is the reducer's comparison shape: 0 for the single-array form, 1 for a variadic
/// call with three or more arguments, 2 for a two-argument call.
fn helper_flags(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    want_max: bool,
    mode: i64,
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
    let mode = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(mode)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(call.span),
    );
    (strict.value, max.value, mode.value)
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

/// Lowers a runtime-typed variadic call to one `Op::MixedMinMax` selection over every operand.
///
/// The whole call is a single instruction because php-src's comparison mode is state that
/// one argument leaves for the following ones (see `MinMaxRule`). Raw `int`/`float` operands
/// and boxed cells are passed as they are, because the selection only reads them. Every
/// other representation (a string, a bool, null, a nullable scalar) is boxed for the call
/// and released after it; the result owns its own reference to the winner's value.
///
/// No operand can hold a GMP object here (`lower` keeps those calls on the reducer), so the
/// comparison runs no PHP code and cannot throw between a box and its release.
fn lower_selection(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    want_max: bool,
) -> LoweredBuiltinValue {
    let rule = MinMaxRule::for_call(want_max, call.operands.len());
    let mut temporaries = Vec::new();
    let operands: Vec<_> = call
        .operands
        .iter()
        .map(|&operand| {
            let representation = ctx.value_php_type(operand).codegen_repr();
            if matches!(
                representation,
                PhpType::Int | PhpType::Float | PhpType::Mixed | PhpType::Union(_)
            ) {
                return operand;
            }
            let boxed = ctx.emit_value(
                Op::MixedBox,
                vec![operand],
                None,
                PhpType::Mixed,
                Op::MixedBox.default_effects(),
                Some(call.span),
            );
            temporaries.push(boxed.value);
            boxed.value
        })
        .collect();
    let selected = ctx.emit_value(
        Op::MixedMinMax,
        operands,
        Some(Immediate::I64(rule.immediate())),
        PhpType::Mixed,
        Op::MixedMinMax.default_effects(),
        Some(call.span),
    );
    for temporary in temporaries {
        ctx.emit_void(
            Op::Release,
            vec![temporary],
            None,
            Op::Release.default_effects(),
            Some(call.span),
        );
    }
    selected
}

/// Lowers `min()` / `max()` to the scalar runtime, a `MixedMinMax` selection or the GMP reducer.
///
/// The reducer is used for the variadic form as soon as one operand may hold GMP, and for
/// the single-array form when the array's elements may hold GMP. It receives one temporary
/// operand array and the comparison mode. A runtime-typed variadic call that cannot meet a
/// GMP object selects through `lower_selection`; everything else keeps the scalar runtime.
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
    // A runtime-typed variadic call that cannot meet a GMP object selects through
    // `MixedMinMax`. One that can keeps the prelude reducer: the GMP comparator is PHP code
    // that may throw, and the reducer's operand array is pinned across that call.
    if !uses_reducer
        && call.operands.len() >= 2
        && matches!(call.result_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        return Ok(lower_selection(ctx, call, want_max));
    }
    if !uses_reducer || (call.operands.len() == 1 && !single_array) {
        return Ok(ctx.emit_runtime_call(
            RuntimeCallTarget::Function(target),
            call.operands.to_vec(),
            call.result_type.clone(),
            Effects::all(),
            Some(call.span),
        ));
    }
    let mode = match call.operands.len() {
        1 => 0,
        2 => 2,
        _ => 1,
    };
    let (strict, max, mode) = helper_flags(ctx, call, want_max, mode);
    if call.operands.len() == 1 {
        let array = call.operands[0];
        let result = ctx.emit_user_call(
            "__elephc_gmp_minmax_array",
            vec![strict, max, mode, array],
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
        vec![strict, max, mode, array.value],
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
