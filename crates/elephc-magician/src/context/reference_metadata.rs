//! Purpose:
//! Defines callable ABI aliases, execution-scope snapshots, reference target
//! shapes, and PHP internal array pointer state.
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

/// PHP argument-introspection metadata for one active eval-declared callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalFunctionArgsFrame {
    regular_params: Vec<String>,
    actual_count: usize,
    surplus: Vec<RuntimeCellHandle>,
    scope: *const ElephcEvalScope,
}

/// One active eval callable as exposed through PHP backtrace functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvalBacktraceFrame {
    function: String,
    arguments: EvalFunctionArgsFrame,
    file: String,
    line: i64,
    class_name: Option<String>,
    object: Option<RuntimeCellHandle>,
    call_type: Option<&'static str>,
}

impl EvalBacktraceFrame {
    /// Creates an active frame with its entry-site and callable-kind metadata.
    pub(crate) fn new(
        function: String,
        arguments: EvalFunctionArgsFrame,
        file: String,
        line: i64,
        class_name: Option<String>,
        object: Option<RuntimeCellHandle>,
        call_type: Option<&'static str>,
    ) -> Self {
        Self {
            function,
            arguments,
            file,
            line,
            class_name,
            object,
            call_type,
        }
    }

    /// Returns the PHP function or method name without a class prefix.
    pub(crate) fn function(&self) -> &str {
        &self.function
    }

    /// Returns the live PHP argument metadata for this activation.
    pub(crate) const fn arguments(&self) -> &EvalFunctionArgsFrame {
        &self.arguments
    }

    /// Returns the source file containing the call that entered this activation.
    pub(crate) fn file(&self) -> &str {
        &self.file
    }

    /// Returns the source line containing the call that entered this activation.
    pub(crate) const fn line(&self) -> i64 {
        self.line
    }

    /// Returns the PHP class name for method and class-bound closure frames.
    pub(crate) fn class_name(&self) -> Option<&str> {
        self.class_name.as_deref()
    }

    /// Returns the borrowed active object for an instance frame.
    pub(crate) const fn object(&self) -> Option<RuntimeCellHandle> {
        self.object
    }

    /// Returns `->` for instance calls, `::` for static calls, or no marker for functions.
    pub(crate) const fn call_type(&self) -> Option<&'static str> {
        self.call_type
    }
}

impl EvalFunctionArgsFrame {
    /// Builds an active frame from fixed parameter names and positional surplus values.
    pub(crate) fn new(
        regular_params: Vec<String>,
        actual_count: usize,
        surplus: Vec<RuntimeCellHandle>,
    ) -> Self {
        Self {
            regular_params,
            actual_count,
            surplus,
            scope: std::ptr::null(),
        }
    }

    /// Associates the frame with the stable stack scope used for this activation.
    pub(crate) fn bind_scope(&mut self, scope: &ElephcEvalScope) {
        self.scope = scope as *const ElephcEvalScope;
    }

    /// Returns the live activation scope associated before the frame was pushed.
    pub(crate) fn scope(&self) -> Option<&ElephcEvalScope> {
        // SAFETY: callable execution binds this pointer after the scope reaches its final
        // stack location, pops the frame before that local is dropped, and never moves the
        // scope while the frame is active.
        unsafe { self.scope.as_ref() }
    }

    /// Returns the number of arguments PHP considers passed to this invocation.
    pub(crate) const fn actual_count(&self) -> usize {
        self.actual_count
    }

    /// Returns the fixed parameter name at one PHP argument position.
    pub(crate) fn regular_param(&self, position: usize) -> Option<&str> {
        self.regular_params.get(position).map(String::as_str)
    }

    /// Returns a positional surplus argument after the fixed parameter prefix.
    pub(crate) fn surplus_arg(&self, position: usize) -> Option<RuntimeCellHandle> {
        self.surplus.get(position).copied()
    }

    /// Returns the number of fixed parameters preceding the positional surplus tail.
    pub(crate) const fn regular_param_count(&self) -> usize {
        self.regular_params.len()
    }
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

/// PHP internal array pointer state tracked per runtime array cell.
///
/// Runtime cells do not carry PHP's `zend_array` internal position, so eval
/// models it as a cursor over the array's iteration order. PHP has exactly one
/// invalid state: once the cursor runs off either end, only `reset()`/`end()`
/// bring it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalArrayCursor {
    /// The pointer addresses one zero-based iteration position.
    Position(usize),
    /// The pointer ran off an end and no longer addresses an element.
    Invalid,
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
