//! Purpose:
//! Home of PHP's `get_called_class` builtin and its late-static-binding EIR lowering.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through the builtin registry.
//!
//! Key details:
//! - Calls outside a class scope are rejected during checking.
//! - Lowering reuses the target-aware `static::class` table lookup on every supported target.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

builtin! {
    contract: "get_called_class",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(Effects::READS_LOCAL),
        result_ownership: BuiltinResultOwnership::Borrowed,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "get_called_class() requires the lexical class scope of its direct call",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Requires a lexical class scope and returns the late-bound class-name type.
fn check(cx: &mut BuiltinCheckCtx<'_>) -> Result<PhpType, CompileError> {
    if cx.checker.current_class.is_none() {
        return Err(CompileError::new(
            cx.span,
            "get_called_class() must be called from within a class",
        ));
    }
    Ok(PhpType::Str)
}

/// Emits the same class-name EIR primitive used by `static::class`.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    if !call.operands.is_empty() {
        return Err(BuiltinLoweringError::new(
            "get_called_class() lowering expected no operands",
        ));
    }
    let data = ctx.intern_class_name("static");
    Ok(ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(call.span),
    ))
}
