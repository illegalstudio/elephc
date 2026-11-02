//! Purpose:
//! Home of PHP's cycle-collector controls — `gc_enabled`, `gc_enable`, `gc_disable` and
//! `gc_collect_cycles` — as the constants an elephc program can honestly answer.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - elephc frees by REFERENCE COUNT and has no cycle collector to turn on, so `gc_enabled()`
//!   is `false`, `gc_collect_cycles()` collects nothing and reports `0`, and the two switches
//!   are no-ops. Reporting `true` instead would be the more "PHP-default-like" answer and the
//!   wrong one: it would promise a collector that is not there.
//! - The whole family has to exist together, because the callers use it as a pair — Symfony's
//!   `AbstractCloner::cloneVar` does `if ($gc = gc_enabled()) { gc_disable(); } … gc_enable();`
//!   — and a program that finds `gc_enabled()` and then no `gc_disable()` fails one line later
//!   than one that finds neither.
//! - Each folds to a constant at compile time; there is no runtime helper to call.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

builtin! {
    contract: "gc_enabled",
    semantics: folded_semantics(lower_gc_enabled),
}

builtin! {
    contract: "gc_enable",
    semantics: folded_semantics(lower_gc_switch),
}

builtin! {
    contract: "gc_disable",
    semantics: folded_semantics(lower_gc_switch),
}

builtin! {
    contract: "gc_collect_cycles",
    semantics: folded_semantics(lower_gc_collect_cycles),
}

// `gc_mem_caches()` returns the bytes the engine's allocator handed back to the system. elephc's
// allocator keeps no such cache, so there is nothing to reclaim; symfony/routing's
// `AttributeFileLoader::load()` calls it after every file it reads.
builtin! {
    contract: "gc_mem_caches",
    semantics: folded_semantics(lower_gc_collect_cycles),
}

/// Builds the shared descriptor for a cycle-collector control that folds to a constant.
const fn folded_semantics(
    lower: crate::builtins::semantics::BuiltinLowerFn,
) -> BuiltinSemantics {
    BuiltinSemantics {
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
            "a cycle-collector control folds to a constant and has no runtime entry point",
        ),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Reports the constant-folding contract: no reads, no writes, no traps.
fn effects(_input: &BuiltinSemanticInput<'_>) -> Effects {
    Effects::PURE
}

/// Folds `gc_enabled()` to `false`: there is no cycle collector to have enabled.
fn lower_gc_enabled(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(false)),
        call.result_type.clone(),
        Effects::PURE,
        Some(call.span),
    ))
}

/// Folds `gc_enable()` / `gc_disable()` to the void placeholder: both are no-ops.
fn lower_gc_switch(
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

/// Folds `gc_collect_cycles()` to `0`: refcounting already freed everything it can.
fn lower_gc_collect_cycles(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(0)),
        call.result_type.clone(),
        Effects::PURE,
        Some(call.span),
    ))
}
