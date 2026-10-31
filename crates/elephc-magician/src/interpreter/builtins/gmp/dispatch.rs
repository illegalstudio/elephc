//! Purpose:
//! Forwards eval calls of the `gmp_*` builtins to the compiled GMP prelude helpers.
//!
//! Called from:
//! - `EvalValuesHook::Gmp` (direct calls, dynamic callables, `call_user_func*`).
//! - `EvalDirectHook::Gmp` when an explicit source-argument hook is selected.
//!
//! Key details:
//! - The public builtin name and the hidden helper suffix are identical by construction
//!   (`gmp_strval` -> `__elephc_gmp_strval`); no per-name dispatch table can drift.
//! - Evaluated builtin arguments arrive in PHP parameter order. The helper adds one
//!   compiler-owned leading `bool $strict`; eval is always weak mode, so it is `false`.
//! - The helper's own defaults, coercions, warnings, and thrown errors are the ones
//!   compiled callers observe, so AOT and eval share one GMP semantics implementation.

use super::*;

/// Returns the compiled GMP prelude helper backing one public `gmp_*` builtin.
fn eval_gmp_host_function(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<NativeFunction, EvalStatus> {
    let helper = format!(
        "__elephc_gmp_{}",
        name.strip_prefix("gmp_").unwrap_or(name)
    );
    match context.native_function(&helper) {
        Some(function) => Ok(function),
        None => eval_throw_error(
            &format!("Call to undefined function {name}()"),
            context,
            values,
        ),
    }
}

/// Invokes one public `gmp_*` builtin with already evaluated arguments.
pub(in crate::interpreter) fn eval_gmp_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function = eval_gmp_host_function(name, context, values)?;
    let strict = values.bool_value(false)?;
    let mut arguments = Vec::with_capacity(evaluated_args.len() + 1);
    arguments.push(EvaluatedCallArg {
        name: None,
        value: strict.borrowed(),
        ref_target: None,
    });
    arguments.extend(evaluated_args.iter().copied().map(|value| EvaluatedCallArg {
        name: None,
        value: value.borrowed(),
        ref_target: None,
    }));
    let result = (|| {
        let bound =
            bind_evaluated_native_function_args(&function, arguments, context, values)?;
        eval_native_function_with_values(function, bound, context, values)
    })();
    finish_eval_argument_values(result, [strict], context, values)
}

/// Evaluates a source-level `gmp_*` call through the shared builtin argument boundary.
pub(in crate::interpreter) fn eval_builtin_gmp_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let arguments = args
        .iter()
        .cloned()
        .map(EvalCallArg::positional)
        .collect::<Vec<_>>();
    eval_builtin_call(name, &arguments, context, scope, values)
}
