//! Purpose:
//! Defines normalized closure targets, capture bindings, and eval closure metadata.
//!
//! Called from:
//! - Closure construction, binding, Reflection, and callable dispatch.
//!
//! Key details:
//! - Bound receivers/scopes and by-reference captures remain explicit runtime metadata.

use super::*;

/// Callable target represented by a PHP-visible eval `Closure` object.
#[derive(Clone)]
pub enum EvalClosureObjectTarget {
    Named(String),
    BoundNamed {
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

/// Runtime value captured by an eval closure literal.
#[derive(Clone)]
pub struct EvalClosureCaptureBinding {
    pub(super) name: String,
    pub(super) value: RuntimeCellHandle,
    pub(super) by_ref_target: Option<EvalReferenceTarget>,
}

impl EvalClosureCaptureBinding {
    /// Creates one captured runtime value with optional caller-side by-reference storage.
    pub fn new(
        name: impl Into<String>,
        value: RuntimeCellHandle,
        by_ref_target: Option<EvalReferenceTarget>,
    ) -> Self {
        Self {
            name: name.into(),
            value,
            by_ref_target,
        }
    }

    /// Returns the captured variable name without the leading `$`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the runtime cell captured by the closure.
    pub const fn value(&self) -> RuntimeCellHandle {
        self.value
    }

    /// Returns caller-side writeback metadata for by-reference captures.
    pub fn by_ref_target(&self) -> Option<&EvalReferenceTarget> {
        self.by_ref_target.as_ref()
    }
}

/// One eval closure instance retained by a synthetic callable name.
#[derive(Clone)]
pub struct EvalClosure {
    pub(super) function: EvalFunction,
    pub(super) captures: Vec<EvalClosureCaptureBinding>,
    pub(super) is_static: bool,
}

impl EvalClosure {
    /// Creates one closure instance from its function body and captured values.
    pub fn new(
        function: EvalFunction,
        captures: Vec<EvalClosureCaptureBinding>,
        is_static: bool,
    ) -> Self {
        Self {
            function,
            captures,
            is_static,
        }
    }

    /// Returns the executable eval function payload for this closure.
    pub fn function(&self) -> &EvalFunction {
        &self.function
    }

    /// Returns the captured runtime values attached to this closure instance.
    pub fn captures(&self) -> &[EvalClosureCaptureBinding] {
        &self.captures
    }

    /// Returns whether this closure was declared with PHP's `static function` form.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }
}
