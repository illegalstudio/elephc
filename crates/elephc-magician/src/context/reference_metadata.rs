//! Purpose:
//! Defines callable ABI aliases, execution-scope snapshots, and reference target shapes.
//!
//! Called from:
//! - Argument binding, reference writeback, object properties, and native invokers.
//!
//! Key details:
//! - Reference targets retain the exact caller-side storage and access scope needed for writeback.

use super::*;

/// Native descriptor-invoker ABI registered by generated code for AOT functions.
pub type NativeFunctionInvoker =
    unsafe extern "C" fn(*mut c_void, *mut RuntimeCell) -> *mut RuntimeCell;

/// Snapshot of eval execution stacks used to restore caller-sensitive access checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElephcEvalExecutionScope {
    pub(super) function_stack: Vec<String>,
    pub(super) class_stack: Vec<String>,
    pub(super) called_class_stack: Vec<String>,
}

/// PHP-visible magic-constant names for the current eval execution frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EvalMagicScope {
    pub(super) function_name: String,
    pub(super) method_name: String,
    pub(super) class_name: String,
    pub(super) trait_name: String,
}

/// Caller-side storage target that can remain linked to an eval object property.
#[derive(Clone)]
pub enum EvalReferenceTarget {
    Variable {
        scope: *mut ElephcEvalScope,
        name: String,
    },
    ArrayElement {
        scope: *mut ElephcEvalScope,
        array_name: String,
        index: RuntimeCellHandle,
    },
    NestedArrayElement {
        array_target: Box<EvalReferenceTarget>,
        index: RuntimeCellHandle,
    },
    ObjectProperty {
        object: RuntimeCellHandle,
        property: String,
        access_scope: ElephcEvalExecutionScope,
    },
    StaticProperty {
        class_name: String,
        property: String,
        access_scope: ElephcEvalExecutionScope,
    },
    Cell {
        cell: RuntimeCellHandle,
    },
    InvokerSlot {
        slot: usize,
        source_tag: u64,
    },
}

/// Normalized PHP array key used for eval-side reference metadata.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EvalArrayReferenceKey {
    Int(i64),
    String(Vec<u8>),
}

/// Late-static dispatch metadata attached to eval-created static callable arrays.
#[derive(Clone)]
pub(super) struct EvalStaticCallableMetadata {
    pub(super) class_name: String,
    pub(super) method: String,
    pub(super) called_class: String,
    pub(super) native_class: Option<String>,
    pub(super) bridge_scope: Option<String>,
}

/// Native instance-method dispatch metadata attached to eval-created method callables.
#[derive(Clone)]
pub(super) struct EvalObjectCallableMetadata {
    pub(super) object: usize,
    pub(super) method: String,
    pub(super) called_class: String,
    pub(super) native_class: String,
    pub(super) bridge_scope: String,
}
