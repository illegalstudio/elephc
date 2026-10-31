//! Purpose:
//! Home of the PHP `floatval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Lowering reuses the general EIR float cast instead of defining a builtin-specific opcode.

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
}

/// Returns the conservative effect contract of the reusable EIR float cast.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::Cast.default_effects()
}

/// Preserves the concrete source representations accepted by runtime callable wrappers.
fn callable_accepts(source: Option<&PhpType>) -> bool {
    source.is_some_and(|source| {
        matches!(
            source.codegen_repr(),
            PhpType::Bool
                | PhpType::Float
                | PhpType::Int
                | PhpType::Never
                | PhpType::Str
                | PhpType::Void
        )
    })
}

/// Lowers `floatval` through the reusable EIR float-cast operation.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let value = call.operand(0)?;
    if matches!(
        ctx.value_php_type(value).codegen_repr(),
        PhpType::Object(name) if name.eq_ignore_ascii_case("GMP")
    ) {
        let method = ctx.intern_string("__elephc_to_float");
        return Ok(ctx.emit_value(
            Op::MethodCall,
            vec![value],
            Some(Immediate::Data(method)),
            call.result_type.clone(),
            Op::MethodCall.default_effects(),
            Some(call.span),
        ));
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
