//! Purpose:
//! Holds small interpreter-local control and call-shape types shared across eval execution modules.
//! These types describe control-flow escape values, evaluated call arguments, and parsed builtin state.
//!
//! Called from:
//! - `crate::interpreter` execution, builtin, and call-dispatch helpers.
//!
//! Key details:
//! - Runtime cells are opaque handles; these types do not own or release values by themselves.

use crate::context::EvalReferenceTarget;
use crate::value::RuntimeCellHandle;

/// Internal statement-control result used to propagate eval returns and loops.
pub(super) enum EvalControl {
    None,
    ReturnVoid,
    Return(RuntimeCellHandle),
    Throw(RuntimeCellHandle),
    Break,
    Continue,
}

/// Final result of executing a parsed eval program.
pub enum EvalOutcome {
    Value(RuntimeCellHandle),
    Throwable(RuntimeCellHandle),
}

/// One already evaluated function-like call argument.
#[derive(Clone)]
pub(super) struct EvaluatedCallArg {
    pub(super) name: Option<String>,
    pub(super) value: RuntimeCellHandle,
    pub(super) ref_target: Option<EvalReferenceTarget>,
}

/// One method argument after PHP parameter-order binding and default materialization.
#[derive(Clone)]
pub(super) struct BoundMethodArg {
    pub(super) value: RuntimeCellHandle,
    pub(super) ref_target: Option<EvalReferenceTarget>,
    pub(super) variadic_ref_targets: Vec<(RuntimeCellHandle, EvalReferenceTarget)>,
}

/// One native function argument list prepared for the descriptor invoker ABI.
pub(super) struct BoundNativeFunctionArgs {
    pub(super) values: Vec<RuntimeCellHandle>,
    pub(super) ref_slots: Vec<BoundNativeFunctionRefSlot>,
}

/// One staged by-reference slot passed to a native function invoker.
pub(super) enum BoundNativeFunctionRefSlot {
    Mixed {
        original: RuntimeCellHandle,
        slot: Box<RuntimeCellHandle>,
        target: Option<EvalReferenceTarget>,
    },
    RawWord {
        tag: u64,
        original: u64,
        slot: Box<u64>,
        target: Option<EvalReferenceTarget>,
    },
    RawString {
        original: [u64; 2],
        slot: Box<[u64; 2]>,
        target: Option<EvalReferenceTarget>,
    },
    OwnedRawWord {
        original: u64,
        slot: Box<u64>,
        target: Option<EvalReferenceTarget>,
    },
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
    Named(String),
    BoundClosure {
        name: String,
        bound_this: RuntimeCellHandle,
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
    Int(i64),
    Float(f64),
    String(&'static str),
}
