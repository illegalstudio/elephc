//! Purpose:
//! Releases directly allocated literal arguments after invocation or failed argument evaluation.
//!
//! Called from:
//! - Object construction, method dispatch, and native function calls in the eval interpreter.
//!
//! Key details:
//! - The shared argument evaluator preserves named/spread order and reference targets.
//! - Literal results have one fresh owner; other expression results retain their existing ownership contract.
//! - Cleanup is explicit because releasing a runtime cell can execute PHP destructors.

use super::*;

/// Runs a call with borrowed literal arguments, then releases their temporary owners on every result.
pub(in crate::interpreter) fn with_literal_call_arguments<V: RuntimeValueOps>(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    invoke: impl FnOnce(
        Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut literal_owners = Vec::new();
    let evaluated = eval_call_arg_values_observed(args, context, scope, values, |expr, value| {
        if matches!(expr, EvalExpr::Const(_)) {
            literal_owners.push(value);
        }
    });
    let result = evaluated.and_then(|args| invoke(args, context, scope, values));
    let mut cleanup = Ok(());
    for value in literal_owners.into_iter().rev() {
        if let Err(status) = values.release(value) {
            cleanup = Err(status);
        }
    }
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = values.release(value);
            Err(status)
        }
    }
}
