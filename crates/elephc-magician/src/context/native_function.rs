//! Purpose:
//! Defines registered native function descriptors and their callable signatures.
//!
//! Called from:
//! - FFI registration and native function binding/dispatch.
//!
//! Key details:
//! - Parameter metadata, bridge support, required arity, and return type travel with the invoker.

use super::*;

/// Native AOT function callback metadata visible to runtime eval fragments.
#[derive(Clone)]
pub struct NativeFunction {
    pub(super) descriptor: *mut c_void,
    pub(super) invoker: NativeFunctionInvoker,
    pub(super) param_count: usize,
    pub(super) param_names: Vec<String>,
    pub(super) param_types: Vec<Option<EvalParameterType>>,
    pub(super) param_defaults: Vec<Option<NativeCallableDefault>>,
    pub(super) param_by_ref: Vec<bool>,
    pub(super) variadic_index: Option<usize>,
    pub(super) return_type: Option<EvalParameterType>,
    pub(super) bridge_supported: bool,
}

impl NativeFunction {
    /// Creates callback metadata for a descriptor-compatible AOT function.
    pub fn new(
        descriptor: *mut c_void,
        invoker: NativeFunctionInvoker,
        param_count: usize,
    ) -> Self {
        Self {
            descriptor,
            invoker,
            param_count,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_defaults: Vec::new(),
            param_by_ref: Vec::new(),
            variadic_index: None,
            return_type: None,
            bridge_supported: true,
        }
    }

    /// Returns the visible positional parameter count accepted by this callback.
    pub const fn param_count(&self) -> usize {
        self.param_count
    }

    /// Records the PHP parameter name for one positional callback slot.
    pub fn set_param_name(&mut self, index: usize, name: impl Into<String>) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_names.len() < self.param_count {
            self.param_names.resize(self.param_count, String::new());
        }
        self.param_names[index] = name.into();
        true
    }

    /// Returns the PHP-visible parameter names registered for this callback.
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }

    /// Records the PHP declared type metadata for one positional callback slot.
    pub fn set_param_type(&mut self, index: usize, param_type: EvalParameterType) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_types.len() < self.param_count {
            self.param_types.resize(self.param_count, None);
        }
        self.param_types[index] = Some(param_type);
        true
    }

    /// Returns PHP declared parameter types registered for this callback.
    pub fn param_types(&self) -> &[Option<EvalParameterType>] {
        &self.param_types
    }

    /// Returns the registered declared type for one parameter slot, if any.
    pub fn param_type(&self, index: usize) -> Option<&EvalParameterType> {
        self.param_types.get(index).and_then(Option::as_ref)
    }

    /// Records a PHP default value for one positional callback slot.
    pub fn set_param_default(&mut self, index: usize, default: NativeCallableDefault) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_defaults.len() < self.param_count {
            self.param_defaults.resize(self.param_count, None);
        }
        self.param_defaults[index] = Some(default);
        true
    }

    /// Returns the registered default for one parameter slot, if any.
    pub fn param_default(&self, index: usize) -> Option<&NativeCallableDefault> {
        self.param_defaults.get(index).and_then(Option::as_ref)
    }

    /// Records whether one positional callback parameter is by-reference.
    pub fn set_param_by_ref(&mut self, index: usize, by_ref: bool) -> bool {
        if index >= self.param_count {
            return false;
        }
        if self.param_by_ref.len() < self.param_count {
            self.param_by_ref.resize(self.param_count, false);
        }
        self.param_by_ref[index] = by_ref;
        true
    }

    /// Records which positional callback parameter is variadic.
    pub fn set_variadic_index(&mut self, index: usize) -> bool {
        if index >= self.param_count {
            return false;
        }
        self.variadic_index = Some(index);
        true
    }

    /// Records the PHP declared return type metadata for this callback.
    pub fn set_return_type(&mut self, return_type: EvalParameterType) {
        self.return_type = Some(return_type);
    }

    /// Records whether eval may dispatch this callback through the generated bridge.
    pub fn set_bridge_supported(&mut self, supported: bool) {
        self.bridge_supported = supported;
    }

    /// Returns whether one registered parameter is by-reference.
    pub fn param_by_ref(&self, index: usize) -> bool {
        self.param_by_ref.get(index).copied().unwrap_or(false)
    }

    /// Returns whether one registered parameter is the variadic parameter.
    pub fn param_variadic(&self, index: usize) -> bool {
        self.variadic_index == Some(index)
    }

    /// Returns the registered declared return type, if any.
    pub fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }

    /// Returns whether eval may dispatch this callback through the generated bridge.
    pub const fn bridge_supported(&self) -> bool {
        self.bridge_supported
    }

    /// Returns the minimum number of required parameters implied by defaults.
    pub fn required_param_count(&self) -> usize {
        if let Some(index) = self.variadic_index {
            return (0..index)
                .rfind(|position| self.param_default(*position).is_none())
                .map_or(0, |position| position + 1);
        }
        (0..self.param_count)
            .rfind(|index| self.param_default(*index).is_none())
            .map_or(0, |index| index + 1)
    }

    /// Invokes the descriptor-compatible callback with a boxed Mixed arg array.
    ///
    /// # Safety
    /// `arg_array` must be a boxed Mixed indexed array whose elements are boxed
    /// Mixed cells following the descriptor-invoker ABI.
    pub unsafe fn call(&self, arg_array: RuntimeCellHandle) -> RuntimeCellHandle {
        RuntimeCellHandle::from_raw((self.invoker)(self.descriptor, arg_array.as_ptr()))
    }
}
