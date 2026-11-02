//! Purpose:
//! Home of the PHP `flush` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's `flush()` pushes the SAPI's own write buffer to the client; it deliberately does
//!   NOT touch userland output buffers, which is `ob_flush()`'s job. elephc has no SAPI write
//!   buffer to push: `--web` assembles the whole response and writes it once at the end of the
//!   request, and a CLI binary writes straight through. PHP documents `flush()` as having no
//!   effect under exactly those conditions, so folding it away is the behavior, not a stub.
//! - It has to EXIST all the same: `Symfony\Component\HttpFoundation\Response::send()` calls
//!   it unconditionally on any SAPI outside `['cli', 'phpdbg', 'embed']`, which is the last
//!   statement of a `--web` request.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    contract: "flush",
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
        callable: BuiltinCallablePolicy::StaticOnly(
            "flush folds away and has no runtime entry point to describe",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Reports the folding contract: no reads, no writes, no traps.
fn effects(_input: &BuiltinSemanticInput<'_>) -> Effects {
    Effects::PURE
}

/// Folds the call to the void placeholder.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstNull,
        Vec::new(),
        None,
        PhpType::Void,
        Effects::PURE,
        Some(call.span),
    ))
}
