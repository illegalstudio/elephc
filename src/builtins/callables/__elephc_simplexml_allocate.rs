//! Purpose:
//! Allocates a prelude object without invoking its user constructor.
//!
//! Called from:
//! - The SimpleXML wrapper factory after parsing or selecting a native XML node.
//!
//! Key details:
//! - Reuses Reflection's existing typed, target-aware constructorless EIR operation.
//! - The returned boxed object is fresh; XML handles are attached by the prelude.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    contract: "__elephc_simplexml_allocate",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly("compiler-prelude object allocation"),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Models allocation, refcounts, and dynamic class lookup failures conservatively.
fn effects(_: &crate::builtins::semantics::BuiltinSemanticInput<'_>) -> Effects {
    Op::DynamicObjectNewWithoutConstructorMixed.default_effects()
}

/// Emits the shared constructorless allocation operation with an owned boxed result.
fn lower(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>)
    -> Result<LoweredBuiltinValue, BuiltinLoweringError>
{
    if call.operands.len() != 1 { return Err(BuiltinLoweringError::new("XML allocation requires a class name")); }
    Ok(ctx.emit_value(Op::DynamicObjectNewWithoutConstructorMixed, call.operands.to_vec(), None,
        PhpType::Mixed, Op::DynamicObjectNewWithoutConstructorMixed.default_effects(),
        Some(call.span)))
}
