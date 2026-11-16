//! Purpose:
//! Home of the internal `__elephc_string_copy` builtin: one owned heap copy of a string.
//!
//! Called from:
//! - Injected preludes that must return a fresh string without using the shared concat
//!   scratch buffer (GMP packet unwrapping is the first consumer).
//!
//! Key details:
//! - Lowers directly to `Op::StrPersist`, which allocates owned heap storage and never
//!   advances `_concat_off`, so nested calls cannot accumulate scratch reservations.
//! - The result is fresh: callers may release any borrowed source immediately.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::Op;
use crate::types::PhpType;

/// Effects of one owned heap copy: reads and allocates heap storage, touches refcounts.
const COPY_EFFECTS: crate::ir::Effects = crate::ir::Effects::READS_HEAP
    .union(crate::ir::Effects::ALLOC_HEAP)
    .union(crate::ir::Effects::REFCOUNT_OP);

builtin! {
    contract: "__elephc_string_copy",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(COPY_EFFECTS),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(|_| true),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Persists the argument into owned heap storage and returns the copy.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::StrPersist,
        vec![call.operand(0)?],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(call.span),
    ))
}
