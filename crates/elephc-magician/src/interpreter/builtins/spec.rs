//! Purpose:
//! Declarative builtin specifications for the eval interpreter.
//! Each spec owns PHP-visible metadata plus optional direct and evaluated-arg
//! dispatch hooks for one builtin.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` lookup and metadata helpers.
//! - `eval_builtin!` submissions in per-builtin home files.
//!
//! Key details:
//! - Specs are collected with `inventory` to let builtin files register
//!   themselves without growing a central match.
//! - Hook enums keep calls monomorphized over `RuntimeValueOps`.

pub(in crate::interpreter) use super::hooks::{EvalDirectHook, EvalValuesHook};
pub(in crate::interpreter) use super::registry::EvalBuiltinDefaultValue;

/// Broad domain used to group eval builtin home files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::interpreter) enum EvalArea {
    /// Array and collection builtins.
    Array,
    /// Filesystem, path, and stream builtins.
    Filesystem,
    /// Formatting and display-oriented numeric builtins.
    Formatting,
    /// JSON encoding, decoding, validation, and error-state builtins.
    Json,
    /// Numeric and mathematical builtins.
    Math,
    /// Network, host, environment, and process builtins.
    NetworkEnv,
    /// PCRE-style regex builtins.
    Regex,
    /// String-processing builtins.
    String,
    /// Date, time, and sleep builtins.
    Time,
    /// Scalar conversion and type-related builtins.
    Types,
}

/// Parameter metadata for one eval builtin argument.
#[derive(Clone, Copy)]
pub(in crate::interpreter) struct EvalParamSpec {
    /// PHP-visible parameter name.
    pub(in crate::interpreter) name: &'static str,
    /// Optional PHP default value.
    pub(in crate::interpreter) default: Option<EvalBuiltinDefaultValue>,
    /// Whether this parameter must bind to caller storage.
    pub(in crate::interpreter) by_ref: bool,
}

/// Static declaration for one PHP-visible eval builtin.
pub(in crate::interpreter) struct EvalBuiltinSpec {
    /// Canonical lowercase PHP builtin name.
    pub(in crate::interpreter) name: &'static str,
    /// Builtin family used by the file layout.
    pub(in crate::interpreter) area: EvalArea,
    /// Parameter names in PHP call order.
    pub(in crate::interpreter) param_names: &'static [&'static str],
    /// Parameter metadata in PHP call order.
    pub(in crate::interpreter) params: &'static [EvalParamSpec],
    /// Variadic parameter name, when supported.
    pub(in crate::interpreter) variadic: Option<&'static str>,
    /// Parameter names that must bind by reference.
    pub(in crate::interpreter) by_ref_params: &'static [&'static str],
    /// Explicit required parameter count for non-trailing default shapes.
    pub(in crate::interpreter) required_param_count: Option<usize>,
    /// Direct expression-level dispatch hook.
    pub(in crate::interpreter) direct: Option<EvalDirectHook>,
    /// Evaluated-argument dispatch hook.
    pub(in crate::interpreter) values: Option<EvalValuesHook>,
}

impl EvalBuiltinSpec {
    /// Returns this builtin's file-layout area.
    pub(in crate::interpreter) fn area(&self) -> EvalArea {
        self.area
    }

    /// Returns the number of required leading parameters.
    pub(in crate::interpreter) fn required_param_count(&self) -> usize {
        if let Some(required_param_count) = self.required_param_count {
            return required_param_count;
        }
        self.params
            .iter()
            .take_while(|param| param.default.is_none())
            .count()
    }

    /// Returns the number of parameters that define defaults.
    pub(in crate::interpreter) fn default_param_count(&self) -> usize {
        let fixed_defaults = self
            .params
            .iter()
            .filter(|param| param.default.is_some())
            .count();
        fixed_defaults + usize::from(self.variadic.is_some())
    }

    /// Returns by-reference parameter names, checking they agree with param flags in debug builds.
    pub(in crate::interpreter) fn by_ref_param_names(&self) -> &'static [&'static str] {
        debug_assert!(self
            .params
            .iter()
            .filter(|param| param.by_ref)
            .all(|param| self.by_ref_params.contains(&param.name)));
        self.by_ref_params
    }

    /// Returns the default value for one PHP parameter slot.
    pub(in crate::interpreter) fn default_value(
        &self,
        param_index: usize,
    ) -> Option<EvalBuiltinDefaultValue> {
        self.params.get(param_index).and_then(|param| param.default)
    }
}

inventory::collect!(EvalBuiltinSpec);
