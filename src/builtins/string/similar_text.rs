//! Purpose:
//! Home of the PHP `similar_text` builtin: its single-source registry declaration and semantic
//! target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The algorithm itself is `__elephc_similar_char`, declared by `crate::similar_text_prelude` in
//!   elephc-PHP; this builtin only validates the call and lowers to a direct call against one of
//!   its two entry points. php's `similar_text()` was absent entirely — `Undefined function` —
//!   and its recursive longest-common-substring scan is the kind of thing that is correct once in
//!   PHP and twice-risky in per-target assembly.
//! - `$percent` IS A SECOND ENTRY POINT, not an optional parameter. A by-reference parameter
//!   cannot be conditionally present in a declaration, so the two arities are two prelude
//!   functions — the shape `crate::scanf_prelude` already uses for the same reason.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// The elephc-PHP prelude entry point for the two-argument form.
const SIMILAR_TEXT_FUNCTION: &str = "__elephc_similar_text";

/// The elephc-PHP prelude entry point that also writes `$percent`.
const SIMILAR_TEXT_PCT_FUNCTION: &str = "__elephc_similar_text_pct";

builtin! {
    contract: "similar_text",
    check: check,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::CheckerHook { check, lazy: false },
        result_type: BuiltinResultType::Checked,
        effects: BuiltinEffects::Shared(engine_call_effects),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "similar_text is counted by an injected prelude function, which a runtime-selected callable cannot reach",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns `Int`, php's count of matching characters.
///
/// The hook exists because the lowering needs the arity decided before it runs, and because a
/// third argument that is not a variable has nowhere to be written: php binds it by reference,
/// and a literal or an expression cannot receive that.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    if let Some(percent) = cx.args.get(2) {
        if !matches!(percent.kind, crate::parser::ast::ExprKind::Variable(_)) {
            return Err(CompileError::new(
                percent.span,
                "similar_text() percent must be a variable, because php writes it by reference",
            ));
        }
        cx.checker.infer_type(percent, cx.env)?;
    }
    Ok(PhpType::Int)
}

/// Returns the effect contract of the call: those of the prelude call it lowers to.
fn engine_call_effects(
    _input: &crate::builtins::semantics::BuiltinSemanticInput<'_>,
) -> crate::ir::Effects {
    Op::Call.default_effects()
}

/// Lowers `similar_text($a, $b, &$percent?)` to a direct call into the injected prelude.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let mut operands = vec![call.operand(0)?, call.operand(1)?];
    let target = if call.operands.len() > 2 {
        operands.push(call.operand(2)?);
        SIMILAR_TEXT_PCT_FUNCTION
    } else {
        SIMILAR_TEXT_FUNCTION
    };
    let name = ctx.intern_function_name(target);
    Ok(ctx.emit_value(
        Op::Call,
        operands,
        Some(Immediate::Data(name)),
        call.result_type.clone(),
        Op::Call.default_effects(),
        Some(call.span),
    ))
}
