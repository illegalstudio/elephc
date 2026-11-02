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
///
/// A `Class::NAME` name is a class constant or an enum case, fetched exactly as the expression
/// `Class::NAME` would be (autoloading the class, enforcing visibility). `defined()` already
/// accepted that form; `constant()` looked it up as a GLOBAL constant and failed, which is what
/// Monolog's `Logger::toMonologLevel()` hit with `\constant(Level::class . '::' . $upper)` on
/// every request of the Symfony demo.
fn eval_constant_lookup(
    name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if let Some((class_name, constant)) = name.split_once("::") {
        // The class-constant fetch hands back the cached cell (an enum case is its singleton)
        // BORROWED, as an expression operand; a builtin's result is owned by its caller, so
        // returning it unretained let the caller's release free the singleton -- the next
        // `constant('TT::T_JOIN')` returned whatever the allocator had put there (Doctrine's DQL
        // lexer typed the second `JOIN` of a query as an identifier).
        let value = eval_class_constant_fetch_result(
            class_name.trim_start_matches('\\'),
            constant,
            context,
            values,
        )?;
        // Only a borrowed cell needs the retain. A constant of a COMPILED class comes back from
        // the native bridge already owned (a fresh box); retaining that too leaked the box and
        // its string on every call (`constant(static::class . '::FOO')` in a loop).
        return if value.is_borrowed() { values.retain(value) } else { Ok(value) };
    }
    eval_const_fetch(&name, context, values)
}
