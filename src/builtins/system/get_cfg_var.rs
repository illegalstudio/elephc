//! Purpose:
//! Home of the PHP `get_cfg_var` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `get_cfg_var()` reads a value set in `php.ini` ONLY, which is a file a compiled elephc
//!   program does not have and never reads; in reference PHP an option set at run time
//!   through `ini_set()` is deliberately invisible to it too. Every option is therefore
//!   "not set here", and PHP's answer for a not-set option is `false` — folded at compile
//!   time rather than asked of a runtime helper that would have nothing to consult.
//! - `ini_get()` is the different question — the ACTIVE value — and keeps its own
//!   implementation. The pair appears together in the wild
//!   (`\ini_get('xdebug.file_link_format') ?: get_cfg_var('xdebug.file_link_format')` in
//!   Symfony's `HtmlErrorRenderer`), which is why answering only one of them left a
//!   `Call to undefined function get_cfg_var()`.
//! - The checked type is `false`, not PHP's declared `array|string|false`: the value is a
//!   compile-time constant, so narrowing it lets `?:` and `=== false` fold. The array form
//!   only ever comes back for an option php.ini set more than once, which cannot happen
//!   when no php.ini is read at all.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op};
use crate::types::PhpType;

builtin! {
    contract: "get_cfg_var",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Shared(eir_result_type),
        effects: BuiltinEffects::Shared(effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "get_cfg_var folds to a constant and has no runtime entry point to describe",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Reports the constant-folding contract: no reads, no writes, no traps.
fn effects(_input: &BuiltinSemanticInput<'_>) -> Effects {
    Effects::PURE
}

/// Returns the backend layout of the folded answer, which is a plain `false`.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::False
}

/// Folds the call to the constant `false` every option answers without a php.ini.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(false)),
        PhpType::False,
        Effects::PURE,
        Some(call.span),
    ))
}

/// Returns `false`, the only answer a program with no php.ini can give.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let Some(arg) = cx.args.first() else {
        return Err(CompileError::new(
            cx.span,
            "get_cfg_var() expects exactly 1 argument",
        ));
    };
    cx.checker.infer_type(arg, cx.env)?;
    Ok(PhpType::False)
}
