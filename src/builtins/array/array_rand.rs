//! Purpose:
//! Home of the PHP `array_rand` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Every AOT call form reaches the injected `__elephc_array_rand` helper. Direct calls and
//!   first-class callables are rewritten by the name resolver; string callables resolve to this
//!   registry builtin and this hook forwards their normalized operands to the same helper. One
//!   implementation, one result representation: the helper returns a boxed `mixed`, which is
//!   PHP's storage for the `int|string|array` union.
//! - `check` validates the array argument and returns that union.

use crate::builtins::requirements::BuiltinRequirement;
use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects,
    BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements,
    BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemanticInput,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

builtin! {
    contract: "array_rand",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[BuiltinRequirement::Bridge("elephc_random")]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        // The runtime callable wrapper forwards its operands to the same injected helper, so a
        // runtime-selected name (`$f = $argv[1] ?? "array_rand"; $f(...)`) reaches one
        // implementation too. The wrapper hands the helper a boxed `mixed` value, and the
        // helper owns PHP's array-argument TypeError.
        callable: BuiltinCallablePolicy::Dynamic(
            crate::builtins::semantics::callable_accepts_any_source,
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the conservative effect contract of the forwarded prelude call.
fn effects(_input: &BuiltinSemanticInput<'_>) -> Effects {
    Op::Call.default_effects()
}

/// Lowers `array_rand()` onto the injected helper that owns the multi-key implementation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let array = call.operand(0)?;
    let num = call.operands.get(1).copied().unwrap_or_else(|| {
        ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(1)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(call.span),
        )
        .value
    });
    Ok(ctx.emit_user_call(
        crate::random_prelude::ARRAY_RAND_HELPER,
        vec![array, num],
        PhpType::Mixed,
        Some(call.span),
    ))
}

/// Validates the array argument and returns PHP's `int|string|array` result union.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_rand() argument must be array",
        ));
    }
    if let Some(num) = cx.args.get(1) {
        cx.checker.infer_type(num, cx.env)?;
    }
    Ok(PhpType::Union(vec![
        PhpType::Int,
        PhpType::Str,
        PhpType::Array(Box::new(PhpType::Mixed)),
    ]))
}
