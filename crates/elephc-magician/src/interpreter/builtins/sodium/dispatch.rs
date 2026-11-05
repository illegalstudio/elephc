//! Purpose:
//! Forwards eval calls of the `sodium_*` builtins to the compiled sodium prelude helpers.
//!
//! Called from:
//! - `EvalValuesHook::Sodium` (direct calls, dynamic callables, `call_user_func*`).
//! - `EvalDirectHook::Sodium` when an explicit source-argument hook is selected.
//!
//! Key details:
//! - The public builtin name and the hidden helper suffix are identical by construction
//!   (`sodium_bin2hex` -> `__elephc_sodium_bin2hex`); no per-name dispatch table can drift.
//! - Evaluated builtin arguments arrive in PHP parameter order. The helper adds one
//!   compiler-owned leading `bool $strict`; eval is always weak mode, so it is `false`.
//! - The helper's own defaults, coercions, warnings, and thrown errors are the ones
//!   compiled callers observe, so AOT and eval share one sodium semantics implementation.

use super::*;

/// Returns the compiled sodium prelude helper backing one public `sodium_*` builtin.
fn eval_sodium_host_function(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<NativeFunction, EvalStatus> {
    let helper = format!(
        "__elephc_sodium_{}",
        name.strip_prefix("sodium_").unwrap_or(name)
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

/// Invokes one public `sodium_*` builtin with already evaluated arguments.
pub(in crate::interpreter) fn eval_sodium_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let function = eval_sodium_host_function(name, context, values)?;
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

/// Returns whether one sodium builtin writes its first argument back through a reference.
fn sodium_mutator(name: &str) -> bool {
    matches!(
        name,
        "sodium_add"
            | "sodium_increment"
            | "sodium_memzero"
            | "sodium_crypto_generichash_update"
            | "sodium_crypto_generichash_final"
            | "sodium_crypto_secretstream_xchacha20poly1305_push"
            | "sodium_crypto_secretstream_xchacha20poly1305_pull"
            | "sodium_crypto_secretstream_xchacha20poly1305_rekey"
    )
}

/// Evaluates a source-level `sodium_*` call through the shared builtin argument boundary.
///
/// By-reference mutators are routed to hidden prelude wrappers whose parameters are
/// ordinary by-reference PHP parameters, so the normal user-function call machinery
/// mutates the caller's variable exactly like AOT.
pub(in crate::interpreter) fn eval_builtin_sodium_expr_call(
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
    if sodium_mutator(name) {
        let wrapper = format!(
            "__elephc_sodium_eval_{}",
            name.strip_prefix("sodium_").unwrap_or(name)
        );
        return eval_call(&wrapper, &arguments, context, scope, values);
    }
    eval_builtin_call(name, &arguments, context, scope, values)
}
