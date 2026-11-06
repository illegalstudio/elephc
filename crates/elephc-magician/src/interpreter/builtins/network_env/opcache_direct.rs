//! Purpose:
//! Binds and dispatches direct and callable invocations of the eight OPcache functions.
//! Prelude and fallback routes use the reference parameter names and internal-function arity.
//! User declarations retain their own signatures and reference semantics.
//!
//! Called from:
//! - `crate::interpreter::expressions::calls::eval_call`.
//! - Callable dispatch for variable calls, first-class callables and `call_user_func[_array]`.
//!
//! Key details:
//! - ONE ROUTE FOR EVERY SPELLING. Each function used to have a direct handler of its own
//!   beside its by-values arm, and the two drifted. The direct handlers read their argument
//!   expressions with `eval_expr` and never released the results, so
//!   `opcache_get_status(str_repeat("x", 70000))` leaked two blocks per call (the PR #968
//!   review measured 20 live blocks after ten calls, and zero through `call_user_func()`).
//!   And they took a spread as one ordinary argument, so `opcache_reset(...[1])` was a fatal
//!   and `opcache_get_status(...[true, false])` was accepted, where reference throws
//!   `ArgumentCountError` for both, as a variable call already did here.
//! - `with_eval_call_arguments` owns the evaluation: source order, spreads unpacked, every
//!   value released on success, refusal or exception.
//! - Named arguments are bound to the reference parameter names, with PHP's three errors:
//!   an unknown name, a name that overwrites a positional argument, and a required parameter
//!   left unpassed (MEASURED on PHP 8.5.10).
//! - THE NATIVE DECLARATION GETS THE SAME BINDING. It is a userland function to the binder,
//!   which accepts surplus positional arguments and answers a missing one with userland's
//!   `Too few arguments`, and it knows no reference parameter names. Since an opaque `eval()`
//!   in a non-default OPcache binary resolves these names to the declarations (see
//!   `opcache_prelude::injection`), `opcache_reset(...[1])` returned normally there and
//!   `opcache_get_status(foo: 1)` was a fatal, where reference throws. Binding here first, and
//!   handing the declaration positional values only, keeps every answer PHP's internal one.

use super::*;

/// Returns internal OPcache parameters only when no user declaration owns the name.
pub(in crate::interpreter) fn eval_opcache_parameters(
    name: &str,
    context: &ElephcEvalContext,
) -> Option<&'static [&'static str]> {
    let parameters = eval_opcache_parameter_names(name)?;
    (!eval_opcache_is_user_function(name, context)).then_some(parameters)
}

/// Identifies OPcache names supplied by eval or native user declarations, rather than preludes.
pub(in crate::interpreter) fn eval_opcache_is_user_function(
    name: &str,
    context: &ElephcEvalContext,
) -> bool {
    eval_opcache_parameter_names(name).is_some()
        && (context.function(name).is_some()
            || context.native_function(name).is_some_and(|function| !function.is_internal()))
}

/// Returns the reference parameter names of an OPcache function, in declaration order.
fn eval_opcache_parameter_names(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "opcache_get_configuration" | "opcache_reset" => &[],
        "opcache_get_status" => &["include_scripts"],
        "opcache_invalidate" => &["filename", "force"],
        "opcache_compile_file"
        | "opcache_is_script_cached"
        | "opcache_is_script_cached_in_file_cache" => &["filename"],
        "opcache_jit_blacklist" => &["closure"],
        _ => return None,
    })
}

/// Evaluates a direct call to the OPcache function `name`: its native declaration when the
/// binary carries one, otherwise the by-values dispatch.
pub(in crate::interpreter) fn eval_opcache_direct_call(
    name: &str,
    parameters: &'static [&'static str],
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |arguments, context, scope, values| {
        eval_opcache_call_with_evaluated_args(
            name, parameters, &arguments, Some(scope), context, values,
        )
    })
}

/// Binds evaluated OPcache arguments before native or fallback dispatch.
/// Argument handles are borrowed; the caller's evaluation lease owns their cleanup.
pub(in crate::interpreter) fn eval_opcache_call_with_evaluated_args(
    name: &str,
    parameters: &[&str],
    arguments: &[EvaluatedCallArg],
    scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bound = eval_bind_opcache_arguments(name, parameters, arguments, context, values)?;
    eval_check_builtin_arity(name, bound.len(), context, values)?;
    if let Some(function) = context.native_function(name) {
        let bound = bind_evaluated_native_function_args(
            &function, positional_args(bound), context, values,
        )?;
        return eval_native_function_with_values(function, bound, context, values);
    }
    eval_builtin_with_values_from_scope(name, &bound, scope, context, values)?
        .ok_or(EvalStatus::UnsupportedConstruct)
}

/// Places positional and named arguments in parameter order.
///
/// Positional arguments keep their count even past the declared parameters, so the arity
/// check that follows reports the number PHP reports (`2 given`).
fn eval_bind_opcache_arguments(
    name: &str,
    parameters: &[&str],
    arguments: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let mut slots: Vec<Option<RuntimeCellHandle>> = Vec::new();
    for argument in arguments {
        let Some(label) = argument.name.as_deref() else {
            slots.push(Some(argument.value));
            continue;
        };
        let Some(index) = parameters.iter().position(|parameter| *parameter == label) else {
            return eval_throw_error(&format!("Unknown named parameter ${label}"), context, values);
        };
        if slots.get(index).is_some_and(Option::is_some) {
            return eval_throw_error(
                &format!("Named parameter ${label} overwrites previous argument"),
                context,
                values,
            );
        }
        if slots.len() <= index {
            slots.resize(index + 1, None);
        }
        slots[index] = Some(argument.value);
    }
    // Every OPcache parameter before the last one passed is required, so a hole is a required
    // parameter that was skipped by naming a later one.
    let mut bound = Vec::with_capacity(slots.len());
    for (index, slot) in slots.into_iter().enumerate() {
        match slot {
            Some(value) => bound.push(value),
            None => {
                let parameter = parameters.get(index).copied().unwrap_or("");
                return eval_throw_argument_count_error(
                    &format!("{name}(): Argument #{} (${parameter}) not passed", index + 1),
                    context,
                    values,
                );
            }
        }
    }
    Ok(bound)
}
