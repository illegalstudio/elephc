//! Purpose:
//! Eval registry entry and implementation for `constant`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Reuses `define`'s constant-name normalizer and the shared dynamic-constant
//!   fetch, so `constant()`, `defined()` and a bare constant reference all resolve
//!   the same name to the same value.
//! - An undefined name is a PHP `Error`; eval reports it as a runtime fatal because
//!   the interpreter has no catchable-throw channel for builtin failures.

use super::define::eval_constant_name;
use super::super::super::*;

eval_builtin! {
    contract: "constant",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates `constant(name)` against eval dynamic and predefined constant names.
pub(in crate::interpreter) fn eval_builtin_constant(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    eval_constant_lookup(name, context, values)
}

/// Evaluates `constant(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_constant_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_constant_lookup(*name, context, values)
}

/// Normalizes one dynamic constant name and returns its retained value.
fn eval_constant_lookup(
    name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    eval_const_fetch(&name, context, values)
}
