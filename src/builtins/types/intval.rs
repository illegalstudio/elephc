//! Purpose:
//! Home of the PHP `intval` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The one-argument form lowers to the general EIR integer cast instead of a
//!   builtin-specific opcode; only the two-argument form needs a runtime target.
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
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::One(RuntimeFnId::IntvalBase),
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the conservative effect contract of the reusable EIR integer cast.
fn effects(_input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    Op::Cast.default_effects()
}

/// Preserves the source representations accepted by runtime callable wrappers.
fn callable_accepts(source: Option<&PhpType>) -> bool {
    source.is_none_or(|source| {
        matches!(
            source.codegen_repr(),
            PhpType::Bool
                | PhpType::Float
                | PhpType::Int
                | PhpType::Mixed
                | PhpType::Never
                | PhpType::Str
                | PhpType::Union(_)
                | PhpType::Void
        )
    })
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
        Some(Immediate::ExplicitCastTarget(IrType::I64)),
        call.result_type.clone(),
        Op::Cast.default_effects(),
        Some(call.span),
    ))
}
