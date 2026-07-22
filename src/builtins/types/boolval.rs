//! Purpose:
//! Home of the PHP `boolval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Lowering reuses PHP truthiness in EIR and records heap reads for container and dynamic values.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Op;
use crate::types::PhpType;

builtin! {
    name: "boolval",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
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
    summary: "Returns the boolean value of a variable.",
    php_manual: "function.boolval",
}

/// Returns the conservative effect contract of the reusable EIR truthiness predicate.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::IsTruthy.default_effects()
}

/// Preserves the concrete source representations accepted by runtime callable wrappers.
fn callable_accepts(source: Option<&PhpType>) -> bool {
    source.is_some_and(|source| {
        matches!(
            source.codegen_repr(),
            PhpType::AssocArray { .. }
                | PhpType::Array(_)
                | PhpType::Bool
                | PhpType::Float
                | PhpType::Int
                | PhpType::Iterable
                | PhpType::Never
                | PhpType::Str
                | PhpType::Void
        )
    })
}

/// Lowers `boolval` through the reusable EIR truthiness predicate.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let operand = call.operand(0)?;
    let input = BuiltinSemanticInput {
        name: call.name,
        args: &[],
        arg_types: &[ctx.value_php_type(operand)],
        span: call.span,
    };
    Ok(ctx.emit_value(
        Op::IsTruthy,
        vec![operand],
        None,
        call.result_type.clone(),
        effects(&input),
        Some(call.span),
    ))
}
