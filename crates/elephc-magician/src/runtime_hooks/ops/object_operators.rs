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
        let mut args: Vec<_> = args.into_iter().map(RuntimeCellHandle::borrowed).collect();
        // A native callback cannot see the evaluator's activation in its AOT backtrace.
        // Carry the caller only across SimpleXML's internal write boundary.
        let eval_method = match method {
            "__elephc_dimension_set" => Some("__elephc_eval_dimension_set"),
            "__elephc_property_set" => Some("__elephc_eval_property_set"),
            _ => None,
        };
        let caller = if eval_method.is_some() {
            let frames = context.backtrace_frames();
            let caller = match frames.first() {
                Some(frame) if frame.function() != "eval" => match frame.class_name() {
                    Some(class) => format!("{class}::{}()", frame.function()),
                    None => format!("{}()", frame.function()),
                },
                _ => "Unknown".to_string(),
            };
            let cell = self.string(&caller)?;
            args.push(cell.borrowed());
            Some(cell)
        } else { None };
        let outcome = interpreter::execute_context_method_call_outcome(context, object.borrowed(),
            eval_method.unwrap_or(method), args, self)
            .and_then(|outcome| Self::operator_outcome(context, outcome));
        if let Some(caller) = caller {
            if let Err(status) = self.release(caller) {
                if let Ok(value) = outcome { self.release(value)?; }
                return Err(status);
            }
        }
        outcome
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
