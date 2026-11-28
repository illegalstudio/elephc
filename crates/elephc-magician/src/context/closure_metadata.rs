//! Purpose:
//! Defines normalized closure targets, capture bindings, and eval closure metadata.
//!
//! Called from:
//! - Closure construction, binding, Reflection, and callable dispatch.
//!
//! Key details:
//! - Bound receivers/scopes and by-reference captures remain explicit runtime metadata.
//! - Captured producer types own only metadata, independently of borrowed PHP value edges.
//! - Foreign PCNTL handler closures retain the eval context that owns their callable metadata.

use super::*;

/// Callable target represented by a PHP-visible eval `Closure` object.
#[derive(Clone)]
pub enum EvalClosureObjectTarget {
    ForeignContext {
        target: Box<EvalClosureObjectTarget>,
        owner: pcntl_runtime::EvalPcntlContextLease,
    },
    NativeDescriptor {
        callback: RuntimeCellHandle,
        display_name: String,
    },
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

impl EvalClosureObjectTarget {
    /// Returns the receiver edge borrowed by this target, descending through foreign-context wrappers.
    pub(crate) fn receiver_mut(&mut self) -> Option<&mut RuntimeCellHandle> {
        match self {
            Self::ForeignContext { target, .. } => target.receiver_mut(),
            Self::NativeDescriptor { callback, .. } => Some(callback),
            Self::BoundNamed { bound_this, .. } => bound_this.as_mut(),
            Self::InvokableObject { object } | Self::ObjectMethod { object, .. } => Some(object),
            Self::Named(_) | Self::StaticMethod { .. } => None,
        }
    }
}

impl EvalClosureObjectTarget {
    /// Returns whether this callable depends on metadata retained by another eval context.
    pub(crate) fn contains_foreign_context(&self) -> bool {
        matches!(self, Self::ForeignContext { .. })
    }
}

/// Runtime value captured by an eval closure literal.
#[derive(Clone)]
pub struct EvalClosureCaptureBinding {
    pub(super) name: String,
    pub(super) value: RuntimeCellHandle,
    pub(super) by_ref_target: Option<EvalReferenceTarget>,
    binding_type: Option<EvalParameterType>,
    source_view: Option<EvalParameterType>,
    source_view_epoch: u64,
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
            binding_type: None,
            source_view: None,
            source_view_epoch: 0,
        }
    }

    /// Attaches an immutable producer contract without retaining its captured PHP value.
    pub(crate) fn with_binding_type(mut self, binding_type: Option<EvalParameterType>) -> Self {
        self.binding_type = binding_type;
        self
    }

    /// Copies a current source view separately from the captured binding's enforcing contract.
    pub(crate) fn with_source_view(mut self, source_view: Option<EvalParameterType>, epoch: u64) -> Self {
        self.source_view = source_view;
        self.source_view_epoch = epoch;
        self
    }

    /// Borrows captured source alternatives only while a shared capture has not received an intervening write.
    pub(crate) fn source_view(&self, context: &ElephcEvalContext) -> Option<&EvalParameterType> {
        if self.by_ref_target.is_some()
            && !context.source_view_is_current(self.value, self.source_view_epoch) { return None; }
        self.source_view.as_ref()
    }

    /// Returns the producer alternatives copied from the defining activation.
    pub(crate) fn binding_type(&self) -> Option<&EvalParameterType> {
        self.binding_type.as_ref()
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
    /// The class whose code created the closure (lowercased), keying its parameter-default
    /// cache: each copy of a trait method creates its own closure op_array in php-src.
    pub(super) creating_class: Option<String>,
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
            creating_class: None,
        }
    }

    /// Records the class whose code created the closure.
    pub fn with_creating_class(mut self, class: Option<&str>) -> Self {
        self.creating_class = class.filter(|class| !class.is_empty()).map(str::to_ascii_lowercase);
        self
    }

    /// Returns the key of this closure's parameter-default cache: its declaration, and the
    /// class whose code created it.
    pub fn default_site(&self) -> String {
        match &self.creating_class {
            Some(class) => format!("{}@{class}", self.function.name()),
            None => self.function.name().to_string(),
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
