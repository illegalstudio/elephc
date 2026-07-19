//! Purpose:
//! Defines native callable defaults and reusable instance/static/constructor signatures.
//!
//! Called from:
//! - FFI registration, argument binding, Reflection, and default materialization.
//!
//! Key details:
//! - Scalar, array, and object defaults preserve keyed/named structure without runtime cells.

use super::*;

/// Default value for a native AOT callable parameter visible to eval fragments.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeCallableDefault {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    EmptyArray,
    Array(Vec<NativeCallableArrayDefaultElement>),
    Object {
        class_name: String,
        args: Vec<NativeCallableObjectDefaultArg>,
    },
}

/// One element in an array-valued native AOT callable default.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCallableArrayDefaultElement {
    pub key: Option<NativeCallableArrayDefaultKey>,
    pub value: NativeCallableDefault,
}

impl NativeCallableArrayDefaultElement {
    /// Creates one auto-indexed element for an array-valued default.
    pub fn positional(value: NativeCallableDefault) -> Self {
        Self { key: None, value }
    }

    /// Creates one explicitly keyed element for an array-valued default.
    pub fn keyed(key: NativeCallableArrayDefaultKey, value: NativeCallableDefault) -> Self {
        Self {
            key: Some(key),
            value,
        }
    }
}

/// Static PHP array key retained for an array-valued native AOT callable default.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeCallableArrayDefaultKey {
    Int(i64),
    String(String),
}

/// Constructor argument for an object-valued native AOT callable default.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCallableObjectDefaultArg {
    pub name: Option<String>,
    pub value: NativeCallableDefault,
}

impl NativeCallableObjectDefaultArg {
    /// Creates one positional constructor argument for an object-valued default.
    pub fn positional(value: NativeCallableDefault) -> Self {
        Self { name: None, value }
    }

    /// Creates one named constructor argument for an object-valued default.
    pub fn named(name: impl Into<String>, value: NativeCallableDefault) -> Self {
        Self {
            name: Some(name.into()),
            value,
        }
    }
}

/// Native AOT method or constructor signature metadata visible to eval fragments.
#[derive(Clone)]
pub struct NativeCallableSignature {
    pub(super) param_count: usize,
    pub(super) param_names: Vec<String>,
    pub(super) param_types: Vec<Option<EvalParameterType>>,
    pub(super) param_defaults: Vec<Option<NativeCallableDefault>>,
    pub(super) param_by_ref: Vec<bool>,
    pub(super) variadic_index: Option<usize>,
    pub(super) return_type: Option<EvalParameterType>,
    pub(super) bridge_supported: bool,
}

impl NativeCallableSignature {
    /// Creates signature metadata with the visible positional parameter count.
    pub const fn new(param_count: usize) -> Self {
        Self {
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

    /// Returns the visible positional parameter count accepted by this callable.
    pub const fn param_count(&self) -> usize {
        self.param_count
    }

    /// Records the PHP parameter name for one positional callable slot.
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

    /// Records the PHP declared type metadata for one positional callable slot.
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

    /// Records a PHP scalar default value for one positional callable slot.
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

    /// Records whether one positional callable parameter is by-reference.
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

    /// Records which positional callable parameter is variadic.
    pub fn set_variadic_index(&mut self, index: usize) -> bool {
        if index >= self.param_count {
            return false;
        }
        self.variadic_index = Some(index);
        true
    }

    /// Records the PHP declared return type metadata for this callable.
    pub fn set_return_type(&mut self, return_type: EvalParameterType) {
        self.return_type = Some(return_type);
    }

    /// Records whether eval may dispatch this callable through the generated bridge.
    pub fn set_bridge_supported(&mut self, supported: bool) {
        self.bridge_supported = supported;
    }

    /// Returns the PHP-visible parameter names registered for this callable.
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }

    /// Returns PHP declared parameter types registered for this callable.
    pub fn param_types(&self) -> &[Option<EvalParameterType>] {
        &self.param_types
    }

    /// Returns the registered declared type for one parameter slot, if any.
    pub fn param_type(&self, index: usize) -> Option<&EvalParameterType> {
        self.param_types.get(index).and_then(Option::as_ref)
    }

    /// Returns the PHP-visible scalar parameter defaults registered for this callable.
    pub fn param_defaults(&self) -> &[Option<NativeCallableDefault>] {
        &self.param_defaults
    }

    /// Returns the registered scalar default for one parameter slot, if any.
    pub fn param_default(&self, index: usize) -> Option<&NativeCallableDefault> {
        self.param_defaults.get(index).and_then(Option::as_ref)
    }

    /// Returns whether one registered parameter is by-reference.
    pub fn param_by_ref(&self, index: usize) -> bool {
        self.param_by_ref.get(index).copied().unwrap_or(false)
    }

    /// Returns whether one registered parameter is the variadic parameter.
    pub fn param_variadic(&self, index: usize) -> bool {
        self.variadic_index == Some(index)
    }

    /// Returns whether eval may dispatch this callable through the generated bridge.
    pub const fn bridge_supported(&self) -> bool {
        self.bridge_supported
    }

    /// Returns the minimum number of arguments required by registered defaults.
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

    /// Returns the registered declared return type metadata, if any.
    pub fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }
}
