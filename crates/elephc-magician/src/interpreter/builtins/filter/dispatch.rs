//! Purpose:
//! Forwards eval calls of the `filter_*` builtins to the compiled filter prelude the host
//! program registered, in every call shape eval has.
//!
//! Called from:
//! - Source-level dispatch (`eval_builtin_filter_call`), declarative direct hooks
//!   (`eval_builtin_filter_expr_call`), by-value callable dispatch
//!   (`eval_filter_values_result`), and the by-reference dynamic-callable arm in
//!   `registry::dynamic_mutation` (`eval_filter_evaluated_call`).
//!
//! Key details:
//! - Every filter function is an ordinary prelude function the host registered as a native
//!   function, so named arguments bind by the host signature and no by-reference output
//!   has to be reconstructed outside `with_eval_call_arguments`.
//! - A host that did not link the bridge has no registered `filter_*` function: the call
//!   raises PHP's catchable `Error: Call to undefined function name()`.

use super::*;

/// Returns whether `name` is one of the filter surface's PHP-visible builtins.
pub(in crate::interpreter) fn eval_filter_builtin_name(name: &str) -> bool {
    elephc_builtin_contract::lookup(name)
        .is_some_and(|contract| contract.module == elephc_builtin_contract::PhpModule::Filter)
}

/// Reports whether an optional filter function is linked in the host.
///
/// `function_exists()` uses this to agree with the compiled program: the contract exists
/// in the shared catalog, but the name is only callable when the host injected the prelude.
pub(in crate::interpreter) fn eval_filter_builtin_linked(
    context: &ElephcEvalContext,
    name: &str,
) -> bool {
    !eval_filter_builtin_name(name)
        || context.native_function(&name.to_ascii_lowercase()).is_some()
}

/// Returns whether the host program registered the compiled filter prelude.
pub(in crate::interpreter) fn eval_filter_bridge_linked(context: &ElephcEvalContext) -> bool {
    context.native_function("filter_var").is_some()
}

/// Returns whether `name` is the filter extension loaded into this eval context.
pub(in crate::interpreter) fn eval_filter_bridge_extension(
    name: &str,
    context: &ElephcEvalContext,
) -> bool {
    name.eq_ignore_ascii_case("filter") && eval_filter_bridge_linked(context)
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
pub(in crate::interpreter) fn eval_builtin_filter_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated, context, _, values| {
        forward(name, evaluated, context, values)
    })
}

/// Evaluates positional expression hooks when registry dispatch is invoked directly.
pub(in crate::interpreter) fn eval_builtin_filter_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_filter_call(name, &args, context, scope, values)
}

/// Evaluates an already-bound callable filter invocation by value.
pub(in crate::interpreter) fn eval_filter_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated = evaluated_args
        .iter()
        .copied()
        .map(|value| EvaluatedCallArg {
            evaluation_owner: None,
            name: None,
            value,
            ref_target: None,
        })
        .collect();
    forward(name, evaluated, context, values)
}

/// Evaluates a dynamic-callable filter invocation, retaining captured callable shape.
pub(in crate::interpreter) fn eval_filter_evaluated_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    forward(name, evaluated_args.to_vec(), context, values)
}
