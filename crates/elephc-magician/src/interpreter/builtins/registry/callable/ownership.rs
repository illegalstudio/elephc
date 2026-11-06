//! Purpose:
//! Owns the receiver acquired while resolving an array callback for invocation.
//! Keeps that owner alive through validation, argument binding and callback execution.
//!
//! Called from:
//! - Callable resolution and the eval builtin/dynamic invocation paths.
//!
//! Key details:
//! - Borrowed receivers never acquire an extra release obligation.
//! - Returned aliases are retained before receiver cleanup; earlier failures keep precedence.

use super::*;

/// A normalized callback plus the receiver owner acquired by an array read.
#[must_use = "finish the callable scope or transfer its receiver owner"]
pub(in crate::interpreter) struct ResolvedEvalCallable {
    pub(super) callable: EvaluatedCallable,
    receiver_owner: Option<RuntimeCellHandle>,
}

impl ResolvedEvalCallable {
    /// Transfers a successful resolver's receiver lease into the invocation scope.
    pub(super) fn new(callable: EvaluatedCallable, receiver_owner: Option<RuntimeCellHandle>) -> Self {
        Self { callable, receiver_owner }
    }

    /// Runs a complete invocation region, then retires its acquired receiver on every status.
    pub(in crate::interpreter) fn with_values<V: RuntimeValueOps>(
        self,
        context: &mut ElephcEvalContext,
        values: &mut V,
        invoke: impl FnOnce(&EvaluatedCallable, &mut ElephcEvalContext, &mut V)
            -> Result<RuntimeCellHandle, EvalStatus>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let result = invoke(&self.callable, context, values);
        finish_eval_argument_values(result, self.receiver_owner, context, values)
    }

    /// Ends a normalization-only probe without transferring its temporary receiver.
    pub(in crate::interpreter) fn release(
        self, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        match self.receiver_owner {
            Some(owner) => release_expr_result(owner, context, values),
            None => Ok(()),
        }
    }

    /// Validates a registration callback while balancing its temporary receiver on failure too.
    #[cfg(feature = "curl")]
    pub(in crate::interpreter) fn with_probe<V: RuntimeValueOps>(
        self, context: &mut ElephcEvalContext, values: &mut V,
        probe: impl FnOnce(&EvaluatedCallable, &mut ElephcEvalContext, &mut V) -> Result<bool, EvalStatus>,
    ) -> Result<bool, EvalStatus> {
        let result = probe(&self.callable, context, values);
        let cleanup = self.release(context, values);
        result.and_then(|exists| cleanup.map(|()| exists))
    }

    /// Transfers the lease to a transfer frame that performs several callback invocations.
    #[cfg(feature = "curl")]
    pub(in crate::interpreter) fn into_parts(self) -> (EvaluatedCallable, Option<RuntimeCellHandle>) {
        (self.callable, self.receiver_owner)
    }

    /// Hands the receiver to Closure::fromCallable's existing target ownership transfer.
    pub(in crate::interpreter) fn into_callable_transferring_receiver(self) -> EvaluatedCallable {
        self.callable
    }
}

/// Keeps an optional array builtin callback leased across its entire iteration.
pub(in crate::interpreter) fn with_optional_evaluated_callable<V: RuntimeValueOps>(
    callback: Option<ResolvedEvalCallable>,
    context: &mut ElephcEvalContext,
    values: &mut V,
    invoke: impl FnOnce(Option<&EvaluatedCallable>, &mut ElephcEvalContext, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = invoke(callback.as_ref().map(|callback| &callback.callable), context, values);
    let owners = callback.as_ref().and_then(|callback| callback.receiver_owner);
    finish_eval_argument_values(result, owners, context, values)
}
