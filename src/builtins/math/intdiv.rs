//! Purpose:
//! Home of the PHP `intdiv` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Numeric argument domains are validated before PHP parameter conversion.
//! - Every call surface retains the shared integer result and catchable value failures.


builtin! {
    contract: "intdiv",
    semantics: semantics(),
}

/// Keeps original argument kinds through prepared direct and callable numeric boundaries.
const fn semantics() -> crate::builtins::semantics::BuiltinSemantics {
    use crate::builtins::semantics::*;
    let mut semantics = eir_runtime_semantics(crate::ir::RuntimeFnId::Intdiv, lower);
    semantics.validation = BuiltinValidation::Shared(validate);
    semantics.argument_lowering = BuiltinArgumentLowering::NumericParameters(
        elephc_semantics::numeric_domains::NumericOperator::Modulo);
    semantics.callable = BuiltinCallablePolicy::Prepared(callable_accepts_any_source);
    semantics
}

/// Rejects known noninteger arguments while dynamic values retain a guarded runtime boundary.
fn validate(input: &crate::builtins::semantics::BuiltinSemanticInput<'_>) -> Result<(), crate::errors::CompileError> {
    let [left, right] = input.arg_types else { return Ok(()); };
    if let Some(rule) = crate::semantics::numeric::operand_rule(input.semantic_profile, input.source_mode,
        &crate::parser::ast::BinOp::Mod) {
        for value in [left, right] {
            crate::semantics::numeric::validate_unary_type(rule, value, input.span)?;
        }
    }
    Ok(())
}

/// Validates borrowed argument tags before the common PHP integer division kernel runs.
fn lower(ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
    call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>)
    -> Result<crate::builtins::semantics::LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    let operator = elephc_semantics::numeric_domains::NumericOperator::Modulo;
    if crate::semantics::numeric::operator_rule(call.semantic_profile, call.source_mode, operator).is_some() {
        ctx.emit_numeric_domain_guard(operator, &call.operands, call.source_mode, call.span)?;
    }
    let target = crate::ir::RuntimeFnId::Intdiv;
    Ok(ctx.emit_runtime_call(crate::ir::RuntimeCallTarget::Function(target), call.operands.to_vec(),
        crate::types::PhpType::Int, target.effects(), Some(call.span)))
}
