//! Purpose:
//! Binds internal object operator calls at the source-to-native method boundary.
//!
//! Called from:
//! - `ElephcRuntimeOps` implementations of the reviewed operator call hooks.
//!
//! Key details:
//! - The shared interpreter binder supplies hidden argument collectors and counts.
//! - Source operands stay borrowed; returned values and pending throws retain their owners.

use super::*;
use crate::abi::ElephcEvalContext;
use crate::interpreter::{self, EvalOutcome};

impl ElephcRuntimeOps {
    /// Calls a static adapter with the same binding rules as an explicit PHP method call.
    pub(super) fn call_static_operator(&mut self, class: &str, method: &str, args: Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
        // The runtime hook owns no context borrow. Generated callbacks already re-enter
        // this active context at the native call boundary through the same pointer.
        let context = unsafe { (self.context as *mut ElephcEvalContext).as_mut() }
            .ok_or(EvalStatus::RuntimeFatal)?;
        let args = args.into_iter().map(RuntimeCellHandle::borrowed).collect();
        let outcome = interpreter::execute_context_static_method_call_outcome(context, class, method, args, self)?;
        Self::operator_outcome(context, outcome)
    }

    /// Calls a boolean adapter after materializing its hidden native parameters.
    pub(super) fn call_instance_operator(&mut self, object: RuntimeCellHandle, method: &str, args: Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
        let context = unsafe { (self.context as *mut ElephcEvalContext).as_mut() }
            .ok_or(EvalStatus::RuntimeFatal)?;
        let args = args.into_iter().map(RuntimeCellHandle::borrowed).collect();
        let outcome = interpreter::execute_context_method_call_outcome(context, object.borrowed(), method, args, self)?;
        Self::operator_outcome(context, outcome)
    }

    /// Transfers a callback exception back to the active evaluator without losing its owner.
    fn operator_outcome(context: &mut ElephcEvalContext, outcome: EvalOutcome) -> Result<RuntimeCellHandle, EvalStatus> {
        match outcome {
            EvalOutcome::Value(value) => Ok(value),
            EvalOutcome::Throwable(thrown) => {
                context.set_pending_throw(thrown);
                Err(EvalStatus::UncaughtThrowable)
            }
        }
    }
}
