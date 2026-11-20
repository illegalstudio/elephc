//! Purpose:
//! Forwards eval locale calls to the host's compiled PHP declarations.
//!
//! Called from:
//! - Eval source-call dispatch and declarative builtin hooks.
//!
//! Key details:
//! - No libc locale state is duplicated in Magician; the host prelude owns it.
//! - Named arguments bind against the host signature, including `setlocale`'s
//!   `$locales` parameter and `...$rest` tail.
//! - A host that did not compile the locale prelude reports an undefined function.

use super::super::*;

mod nl_langinfo;
mod setlocale;

/// Recognizes the exact PHP locale surface declared by the locale prelude.
pub(in crate::interpreter) fn eval_locale_builtin_name(name: &str) -> bool {
    matches!(name, "setlocale" | "nl_langinfo")
}

/// Reports whether the host compiled the requested locale function.
pub(in crate::interpreter) fn eval_locale_builtin_linked(
    context: &ElephcEvalContext,
    name: &str,
) -> bool {
    !eval_locale_builtin_name(name)
        || context.native_function(&name.to_ascii_lowercase()).is_some()
}

/// Forwards evaluated arguments through the shared named/positional native binder.
fn forward(
    name: &str,
    args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(function) = context.native_function(&name.to_ascii_lowercase()) else {
        return eval_throw_error(&format!("Call to undefined function {name}()"), context, values);
    };
    let bound = bind_evaluated_native_function_args(&function, args, context, values)?;
    eval_native_function_with_values(function, bound, context, values)
}

/// Keeps named arguments intact when forwarding an eval source call.
pub(in crate::interpreter) fn eval_builtin_locale_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |args, context, _, values| {
        forward(name, args, context, values)
    })
}

/// Adapts positional expression hooks to the shared source-call path.
pub(in crate::interpreter) fn eval_builtin_locale_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_locale_call(name, &args, context, scope, values)
}

/// Forwards dynamic callable invocations after their arguments have been evaluated.
pub(in crate::interpreter) fn eval_locale_values_result(
    name: &str,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args
        .iter()
        .map(|value| EvaluatedCallArg {
            source_type: None,
            evaluation_owner: None,
            name: None,
            value: *value,
            ref_target: None,
        })
        .collect();
    forward(name, args, context, values)
}
