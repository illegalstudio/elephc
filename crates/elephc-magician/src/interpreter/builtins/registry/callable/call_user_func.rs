//! Purpose:
//! Forwards named call_user_func arguments to its callback without binding them to the wrapper.
//!
//! Called from:
//! - `registry::owned_arguments::with_owned_builtin_arguments()` after source-order evaluation.
//!
//! Key details:
//! - Only the callback parameter belongs to call_user_func; variadic names belong to its target.
//! - Argument handles are borrowed from the wrapper's owner lease and use by-value semantics.

use super::*;

/// Selects the wrapper callback and preserves every remaining argument's name for its target.
pub(in crate::interpreter) fn eval_call_user_func_with_call_args_from_scope(
    arguments: Vec<EvaluatedCallArg>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut callback = None;
    let mut callback_args = Vec::new();
    for argument in arguments {
        if argument.name.as_deref() == Some("callback")
            || (argument.name.is_none() && callback.is_none())
        {
            if callback.is_some() {
                return eval_throw_error(
                    "Named parameter $callback overwrites previous argument", context, values,
                );
            }
            callback = Some(argument.value);
        } else {
            callback_args.push(argument);
        }
    }
    let Some(callback) = callback else {
        eval_check_builtin_arity("call_user_func", 0, context, values)?;
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback_is_object = values.type_tag(callback)? == EVAL_TAG_OBJECT;
    let callback =
        eval_call_user_func_callback(callback, "call_user_func", lexical_scope, context, values)?;
    callback.with_values(context, values, |callback, context, values| {
        if let EvaluatedCallable::Named { name, .. } = &callback {
            if let Some(forbidden) = eval_forbidden_dynamic_scope_builtin(name) {
                if forbidden != "get_defined_vars" || callback_is_object {
                    return eval_throw_forbidden_dynamic_scope_builtin(forbidden, context, values);
                }
            }
            if name.eq_ignore_ascii_case("get_defined_vars") {
                if let Some(lexical_scope) = lexical_scope {
                    if let Some(label) = callback_args.iter().find_map(|arg| arg.name.as_deref()) {
                        return eval_throw_error(
                            &format!("Unknown named parameter ${label}"), context, values,
                        );
                    }
                    let positional = callback_args.iter().map(|arg| arg.value).collect::<Vec<_>>();
                    return eval_get_defined_vars_from_scope(&positional, lexical_scope, values);
                }
            }
        }
        eval_evaluated_callable_with_by_value_call_args(&callback, callback_args, context, values)
    })
}
