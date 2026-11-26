//! Purpose:
//! Holds small interpreter-local control and call-shape types shared across eval execution modules.
//! These types describe control-flow escape values, evaluated call arguments, and parsed builtin state.
//!
//! Called from:
//! - `crate::interpreter` execution, builtin, and call-dispatch helpers.
//!
//! Key details:
//! - Runtime cells are opaque handles; only foreign PCNTL callables own a
//!   temporary lease that keeps their eval context alive through dispatch.

use crate::context::{pcntl_runtime::EvalPcntlContextLease, EvalReferenceTarget};
use crate::value::RuntimeCellHandle;
use elephc_memoir::{
    owner_layout::InvokerSlotWords,
    runtime_invoker_slots::{InvokerMixedSlot, InvokerSlot, InvokerValidationPin},
    runtime_owners::RuntimeOwners,
};
use std::pin::Pin;
use super::argument_value_owners::ArgumentValueOwner;

/// Internal statement-control result used to propagate eval returns and loops.
pub(super) enum EvalControl {
    None,
    ReturnVoid,
    Return(RuntimeCellHandle),
    Throw(RuntimeCellHandle),
    Break,
    Continue,
    /// A `goto` on its way to the statement list that holds its label.
    Goto(String),
}

impl RuntimeOwners for EvalControl {
    /// Publishes only the runtime cell transported by return or throw control flow.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut crate::value::RuntimeCell)) {
        match self {
            Self::Return(value) | Self::Throw(value) => value.visit_owned_slots(visit),
            Self::None
            | Self::ReturnVoid
            | Self::Break
            | Self::Continue
            | Self::Goto(_) => {}
        }
    }
}

/// Final result of executing a parsed eval program.
pub enum EvalOutcome {
    Value(RuntimeCellHandle),
    Throwable(RuntimeCellHandle),
}

/// One already evaluated function-like call argument.
#[derive(Clone)]
pub(super) struct EvaluatedCallArg {
    /// Exact source-evaluation lease, preserved through binding without copying its owner.
    pub(super) evaluation_owner: Option<ArgumentValueOwner>,
    pub(super) name: Option<String>,
    pub(super) value: RuntimeCellHandle,
    pub(super) ref_target: Option<EvalReferenceTarget>,
}

impl RuntimeOwners for EvaluatedCallArg {
    /// Publishes a completed argument's source lease, value and owned reference metadata.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut crate::value::RuntimeCell)) {
        self.evaluation_owner.visit_owned_slots(visit);
        self.value.visit_owned_slots(visit);
        self.ref_target.visit_owned_slots(visit);
    }
}

/// One method argument after PHP parameter-order binding and default materialization.
#[derive(Clone)]
pub(super) struct BoundMethodArg {
    pub(super) value: RuntimeCellHandle,
    pub(super) ref_target: Option<EvalReferenceTarget>,
    pub(super) variadic_ref_targets: Vec<(RuntimeCellHandle, EvalReferenceTarget)>,
}

/// One native function argument list prepared for the descriptor invoker ABI.
#[derive(Default)]
pub(super) struct BoundNativeFunctionArgs {
    /// Source argument leases can become invisible only after staging and coercion finish.
    pub(super) evaluation_owners: Vec<ArgumentValueOwner>,
    pub(super) values: Vec<RuntimeCellHandle>,
    pub(super) ref_slots: Vec<BoundNativeFunctionRefSlot>,
    /// Container key of each staged value: `None` is its own position, `Some(name)` a string key.
    ///
    /// Only a SOURCE-declared variadic can absorb an unknown named argument, and PHP keeps that
    /// argument's string key in the collected array. Recording the key here is what lets the
    /// container become associative instead of losing the name, and it stays index-aligned with
    /// `values` because staging pushes exactly one value per bound argument, in order.
    pub(super) named_keys: Vec<Option<String>>,
}

impl RuntimeOwners for BoundNativeFunctionArgs {
    /// Publishes source leases, staged values and reference payloads as one native activation.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut crate::value::RuntimeCell)) {
        self.evaluation_owners.visit_owned_slots(visit);
        self.values.visit_owned_slots(visit);
        self.ref_slots.visit_owned_slots(visit);
    }
}

/// One staged by-reference slot passed to a native function invoker.
pub(super) enum BoundNativeFunctionRefSlot {
    /// One managed native slot owns the live PHP reference shared with the caller.
    ManagedMixed {
        original: RuntimeCellHandle,
        cell: u64,
        /// Stable foreign-mutable record remains allocated until native pin retirement completes.
        pin: InvokerValidationPin,
    },
    Mixed {
        original: RuntimeCellHandle,
        slot: InvokerMixedSlot,
        target: Option<EvalReferenceTarget>,
    },
    RawWord {
        tag: u64,
        original: u64,
        slot: Pin<Box<InvokerSlot>>,
        target: Option<EvalReferenceTarget>,
        force_writeback: bool,
    },
    RawString {
        original: InvokerSlotWords,
        slot: Pin<Box<InvokerSlot>>,
        target: Option<EvalReferenceTarget>,
        force_writeback: bool,
    },
    TaggedScalar {
        original: InvokerSlotWords,
        slot: Pin<Box<InvokerSlot>>,
        target: Option<EvalReferenceTarget>,
    },
    OwnedRawWord {
        original: u64,
        slot: Pin<Box<InvokerSlot>>,
        target: Option<EvalReferenceTarget>,
    },
}

impl RuntimeOwners for BoundNativeFunctionRefSlot {
    /// Exposes detached Mixed owners; managed pins are visited by the native chain.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut crate::value::RuntimeCell)) {
        if let Self::Mixed { slot, .. } = self {
            slot.visit_owned_slots(visit);
        }
    }
}

/// How a callable binder should handle by-reference parameters without caller storage.
#[derive(Clone, Copy)]
pub(super) enum EvalByRefBindingMode<'a> {
    RequireTarget,
    WarnByValue {
        callable_name: &'a str,
    },
}

/// One already evaluated PHP callback supported by the eval dispatcher.
pub(super) enum EvaluatedCallable {
    ForeignContext {
        callback: Box<EvaluatedCallable>,
        owner: EvalPcntlContextLease,
    },
    NativeDescriptor {
        callback: RuntimeCellHandle,
        display_name: String,
    },
    Named {
        name: String,
        display_name: String,
    },
    BoundClosure {
        name: String,
        bound_this: Option<RuntimeCellHandle>,
        bound_scope: Option<String>,
    },
    InvokableObject {
        object: RuntimeCellHandle,
    },
    ObjectMethod {
        object: RuntimeCellHandle,
        method: String,
        called_class: Option<String>,
        native_class: Option<String>,
        bridge_scope: Option<String>,
    },
    StaticMethod {
        class_name: String,
        method: String,
        called_class: Option<String>,
        native_class: Option<String>,
        bridge_scope: Option<String>,
    },
}

/// Bound argument tuple for direct `array_splice()` calls.
pub(super) type EvalArraySpliceDirectArgs = (
    RuntimeCellHandle,
    EvalReferenceTarget,
    RuntimeCellHandle,
    Option<RuntimeCellHandle>,
    Option<RuntimeCellHandle>,
);

/// Parsed flags for one eval `sprintf()` conversion specifier.
#[derive(Clone, Copy)]
pub(super) struct EvalSprintfSpec {
    pub(super) left_align: bool,
    pub(super) force_sign: bool,
    pub(super) space_sign: bool,
    pub(super) zero_pad: bool,
    pub(super) alternate: bool,
    pub(super) width: Option<usize>,
    pub(super) precision: Option<usize>,
    pub(super) specifier: u8,
}

/// Eval-visible predefined constant payloads that are not stored in the dynamic context.
pub(super) enum EvalPredefinedConstant {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(&'static str),
}
