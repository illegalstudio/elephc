//! Purpose:
//! Home of the PHP `settype` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The first parameter `var` is passed by reference (mutating builtin); `ref_params[0]`
//!   is set by the `ref` marker in the `builtin!` declaration.
//! - `lazy_check: true` so the check hook controls argument inference order: it infers
//!   `var` then `type` in source order (once each), matching legacy exactly-once inference.
//! - `check` validates that the second argument is a string and returns `Bool`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "settype",
    check: check,
    lazy_check: true,
    semantics: semantics(),
}

/// Keeps the ordinary typed backend while giving checked callable bodies the full EIR context.
const fn semantics() -> crate::builtins::semantics::BuiltinSemantics {
    use crate::builtins::semantics::*;
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::Settype);
    semantics.requirements = BuiltinRequirements::Static(&[
        BuiltinRequirement::PreludeHelper(crate::object_cast_prelude::CAST_HELPER),
        BuiltinRequirement::PreludeHelper(crate::object_cast_prelude::DYNAMIC_CAST_HELPER),
    ]);
    semantics.target_strategy = BuiltinTargetStrategy::EirGraph;
    semantics.effects = BuiltinEffects::Shared(effects);
    semantics.callable = BuiltinCallablePolicy::Prepared(callable_accepts_any_source);
    semantics.lowering = BuiltinLowering::Eir(lower);
    semantics
}

/// String conversion may invoke user code and mutate globals inside the checked value graph.
fn effects(input: &crate::builtins::semantics::BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    if crate::semantics::conversions::checked_casts(input.semantic_profile, input.source_mode) {
        crate::ir::Effects::from_bits_retain(crate::ir::Effects::all().bits()
            & !crate::ir::Effects::REFCOUNT_OP.bits() & !crate::ir::Effects::BLOCKING_IO.bits()
            & !crate::ir::Effects::NETWORK_IO.bits())
    } else { crate::ir::RuntimeFnId::Settype.effects() }
}

/// Shares evaluated original values and reference storage between direct and callable checked paths.
fn lower(ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
    call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>)
    -> Result<crate::builtins::semantics::LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    if crate::semantics::conversions::checked_casts(call.semantic_profile, call.source_mode)
        || ctx.uses_prepared_builtin_semantics() {
        return ctx.emit_settype_conversion(call.operand(0)?, call.operand(1)?, call.source_mode, call.span);
    }
    let target = crate::ir::RuntimeFnId::Settype;
    Ok(ctx.emit_runtime_call(crate::ir::RuntimeCallTarget::Function(target), call.operands.to_vec(),
        call.result_type.clone(), target.effects(), Some(call.span)))
}

/// Validates the `settype` arguments: infers both in source order and rejects a non-string type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let source = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if ty != PhpType::Str {
        return Err(CompileError::new(
            cx.span,
            "settype() second argument must be a string",
        ));
    }
    if let crate::parser::ast::ExprKind::StringLiteral(name) = &cx.args[1].kind {
        if crate::semantics::conversions::checked_casts(cx.checker.semantic_profile, cx.source_mode) {
            elephc_semantics::settype_target::SettypeTarget::parse(name)
                .map_err(|failure| CompileError::new(cx.span, failure.message()))?;
        }
        if let Some(target) = elephc_semantics::numeric_cast::NumericCastTarget::from_type_name(name) {
            let target = match target {
                elephc_semantics::numeric_cast::NumericCastTarget::Int => crate::parser::ast::CastType::Int,
                elephc_semantics::numeric_cast::NumericCastTarget::Float => crate::parser::ast::CastType::Float,
            };
            cx.checker.validate_explicit_numeric_cast(cx.source_mode, &target, &source, cx.span)?;
        }
    }
    Ok(PhpType::Bool)
}
