//! Purpose:
//! Home of PHP's `get_class_methods` builtin and its AOT checker contract.
//!
//! Called from:
//! - Checker, optimizer, ownership, and class-introspection specialization through the registry.
//!
//! Key details:
//! - AOT accepts an object or a runtime class-name string.
//! - Mixed arguments are validated against their runtime tag before metadata lookup.
//! - Direct, spread, and statically resolved callable paths use the AOT metadata specializer.
//! - The specializer composes GetClass and Explode with their own logical signatures.
//!   Neither runtime ID aliases this builtin's one-argument signature in the registry.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics,
    BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue,
    NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::ir::Effects;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "get_class_methods",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::Shared(validate),
        result_type: BuiltinResultType::Shared(result_type),
        effects: BuiltinEffects::Static(Effects::from_bits_retain(
            Effects::READS_HEAP.bits()
                | Effects::ALLOC_HEAP.bits()
                | Effects::MAY_THROW.bits(),
        )),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "get_class_methods() requires a statically resolved callable target in AOT mode",
        ),
        lowering: BuiltinLowering::Eir(lower_unreachable),
    },
}

/// Accepts static or boxed object/string arguments and defers unpacked entry validation to runtime binding.
fn validate(input: &BuiltinSemanticInput<'_>) -> Result<(), CompileError> {
    if input.args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        return Ok(());
    }
    if !input.arg_types.first().is_some_and(|ty| {
        matches!(
            ty.codegen_repr(),
            PhpType::Object(_) | PhpType::Str | PhpType::Mixed | PhpType::Union(_)
        )
    }) {
        return Err(CompileError::new(
            input.span,
            "get_class_methods() argument must be an object or string in AOT mode",
        ));
    }
    Ok(())
}

/// Shares the concrete indexed-string result layout between checker and EIR consumers.
fn result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Array(Box::new(PhpType::Str))
}

/// Rejects any path that bypassed the AOT class-metadata specialization.
fn lower_unreachable(
    _ctx: &mut dyn BuiltinLoweringContext,
    _call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(
        "get_class_methods() bypassed its statically resolved EIR specialization",
    ))
}
