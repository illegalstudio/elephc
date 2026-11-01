//! Purpose:
//! Home of PHP's `set_time_limit()` — the execution-time control, folded to the `true` php
//! always returns.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - php's return value is unconditionally `true` on this build: every probed argument -- `0`,
//!   `30`, `-1`, `PHP_INT_MAX`, `true`, `"5"`, `null` -- answered `bool(true)` on php 8.5.10 CLI.
//!   No input was found that returns `false`, so the fold is exact for the return value.
//! - `$seconds <= 0` means "no limit" in php, and an elephc binary has no limit either, so those
//!   calls are FULLY faithful. Symfony's runtime bootstrap calls `set_time_limit(0)`, which is
//!   that case.
//! - A POSITIVE `$seconds` is a NAMED DIVERGENCE: php CLI really arms the timer
//!   (`php -r 'set_time_limit(1); $t=microtime(true); while (microtime(true)-$t < 3.0) {}'` dies
//!   with `Fatal error: Maximum execution time of 1 second exceeded`), and elephc has no
//!   execution-time interrupt to arm. A compiled program therefore runs past the limit instead
//!   of aborting. Reporting `false` instead of `true` would not make that honest -- it would
//!   just make the RETURN value wrong as well -- so the no-op returns what php returns and the
//!   divergence is documented here and on the builtin's doc page.
//! - A SECOND named divergence: in php the new value is visible through
//!   `ini_get('max_execution_time')` (measured: `"30"` after `set_time_limit(30)`). elephc's
//!   `ini_get` is the compile-time table `__elephc_opcache_ini_string`, and its `ini_set`
//!   already reports failure for every key, so no runtime ini store exists for this to write
//!   into. `max_execution_time` is absent from that table entirely.
//! - Folded at compile time like the `gc_*` family: there is no runtime helper to call. The
//!   argument expression is lowered BEFORE this fold runs, so its own side effects survive; only
//!   the resulting value is discarded.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op};

builtin! {
    contract: "set_time_limit",
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
            "set_time_limit() folds to a constant and has no runtime entry point",
        ),
        lowering: BuiltinLowering::Eir(lower_set_time_limit),
    },
}

/// Reports the constant-folding contract: no reads, no writes, no traps.
///
/// php's `set_time_limit()` does mutate `max_execution_time`, but elephc has no runtime ini
/// store for it to mutate and no `ini_get` that could observe one, so there is no global state
/// here to declare — claiming a write would only block optimizations for a value nothing reads.
fn effects(_input: &BuiltinSemanticInput<'_>) -> Effects {
    Effects::PURE
}

/// Folds `set_time_limit()` to `true`, the only value php was measured to return.
fn lower_set_time_limit(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(true)),
        call.result_type.clone(),
        Effects::PURE,
        Some(call.span),
    ))
}
