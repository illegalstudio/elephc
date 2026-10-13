//! Purpose:
//! Saves callable/reflection flow facts while lowering a terminal alternate cleanup path.
//!
//! Called from:
//! - The argument-evaluation guard after its success path has been lowered.
//!
//! Key details:
//! - Capture occurs after real argument effects and their invalidations, never before evaluation.
//! - Restoration does not rewind IR, ownership ledgers, slot state or mutation generations.
//! - The alternate cleanup path rethrows and cannot join the captured success continuation.

use super::{Expr, FunctionSig, HashMap, LoweringContext, StaticCallableBinding};

/// Exact callable/reflection associations valid on an already-lowered success path.
pub(crate) struct CallableFlowFacts {
    callable: HashMap<String, StaticCallableBinding>,
    reflection_class: HashMap<String, String>,
    reflection_function: HashMap<String, String>,
    reflection_property: HashMap<String, (String, String)>,
    reflection_method: HashMap<String, (String, String)>,
    reflection_args: HashMap<String, Vec<Expr>>,
    fiber_start: HashMap<String, FunctionSig>,
}

impl LoweringContext<'_, '_> {
    /// Captures the actual post-evaluation facts before emitting terminal cleanup code.
    pub(crate) fn snapshot_callable_flow_facts(&self) -> CallableFlowFacts {
        CallableFlowFacts {
            callable: self.static_callable_locals.clone(),
            reflection_class: self.reflection_class_locals.clone(),
            reflection_function: self.reflection_function_locals.clone(),
            reflection_property: self.reflection_property_locals.clone(),
            reflection_method: self.reflection_method_locals.clone(),
            reflection_args: self.reflection_arg_array_locals.clone(),
            fiber_start: self.fiber_start_sigs.clone(),
        }
    }

    /// Restores success-path associations without erasing conservative mutation generations.
    pub(crate) fn restore_callable_flow_facts(&mut self, facts: CallableFlowFacts) {
        self.static_callable_locals = facts.callable;
        self.reflection_class_locals = facts.reflection_class;
        self.reflection_function_locals = facts.reflection_function;
        self.reflection_property_locals = facts.reflection_property;
        self.reflection_method_locals = facts.reflection_method;
        self.reflection_arg_array_locals = facts.reflection_args;
        self.fiber_start_sigs = facts.fiber_start;
    }
}
