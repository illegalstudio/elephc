//! Purpose:
//! Magician implementation bindings and assembled builtin specifications.
//! Shared contracts own PHP-visible metadata; each binding contributes only
//! eval-specific grouping and dispatch hooks.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` lookup and metadata helpers.
//! - `eval_builtin!` submissions in per-builtin home files.
//!
//! Key details:
//! - Bindings are collected with `inventory` and joined to neutral contracts
//!   once when the runtime registry initializes.
//! - Hook enums keep calls monomorphized over `RuntimeValueOps`.

use elephc_builtin_contract::{
    eval_execution, eval_signature, lookup_id, BuiltinId, DefaultSpec, EvalExecution, ParamSpec,
    RuntimeBuiltinId,
};

pub(in crate::interpreter) use super::hooks::{EvalDirectHook, EvalValuesHook};
pub(in crate::interpreter) use super::registry::EvalBuiltinDefaultValue;

/// Broad domain used to group eval builtin home files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::interpreter) enum EvalArea {
    /// Array and collection builtins.
    Array,
    /// Core callable, constant, process-control, and debug-output builtins.
    Core,
    /// `ext/curl` builtins (behind the `curl` Cargo feature; see
    /// `crate::interpreter::builtins::curl`'s module doc). Gated the same as that module:
    /// no home file constructs this variant without the feature, so an unconditional
    /// variant would be permanently dead code in the default build.
    #[cfg(feature = "curl")]
    Curl,
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
    /// Raw pointer and buffer extension builtins.
    RawMemory,
    /// String-processing builtins.
    String,
    /// Symbol, class metadata, SPL, and language-construct probes.
    Symbols,
    /// Date, time, and sleep builtins.
    Time,
    /// Scalar conversion and type-related builtins.
    Types,
}

impl EvalArea {
    /// Returns the stable lowercase spelling used by documentation metadata.
    pub(in crate::interpreter) fn name(self) -> &'static str {
        match self {
            EvalArea::Array => "array",
            EvalArea::Core => "core",
            #[cfg(feature = "curl")]
            EvalArea::Curl => "curl",
            EvalArea::Filesystem => "filesystem",
            EvalArea::Formatting => "formatting",
            EvalArea::Json => "json",
            EvalArea::Math => "math",
            EvalArea::NetworkEnv => "network_env",
            EvalArea::Regex => "regex",
            EvalArea::RawMemory => "raw_memory",
            EvalArea::String => "string",
            EvalArea::Symbols => "symbols",
            EvalArea::Time => "time",
            EvalArea::Types => "types",
        }
    }
}

/// Parameter metadata materialized for one eval builtin argument.
#[derive(Clone, Copy)]
pub(in crate::interpreter) struct EvalParamSpec {
    /// PHP-visible parameter name.
    pub(in crate::interpreter) name: &'static str,
    /// Optional PHP default value in Magician's runtime representation.
    pub(in crate::interpreter) default: Option<EvalBuiltinDefaultValue>,
    /// Whether this parameter must bind to caller storage.
    pub(in crate::interpreter) by_ref: bool,
}

/// Magician-specific implementation binding submitted by one builtin home file.
pub(in crate::interpreter) struct EvalBuiltinBinding {
    /// Stable shared-contract identity.
    pub(in crate::interpreter) id: BuiltinId,
    /// Builtin family used by the file layout.
    pub(in crate::interpreter) area: EvalArea,
    /// Direct expression-level dispatch hook.
    pub(in crate::interpreter) direct: Option<EvalDirectHook>,
    /// Evaluated-argument dispatch hook.
    pub(in crate::interpreter) values: Option<EvalValuesHook>,
    /// Workspace-relative path of the home file that declared this builtin.
    pub(in crate::interpreter) home_file: &'static str,
}

/// Runtime-ready join of one shared contract and one Magician binding.
pub(in crate::interpreter) struct EvalBuiltinSpec {
    /// Canonical lowercase PHP builtin name.
    pub(in crate::interpreter) name: &'static str,
    /// Builtin family used by the file layout.
    pub(in crate::interpreter) area: EvalArea,
    /// Parameter names in PHP call order, including a variadic tail.
    pub(in crate::interpreter) param_names: Box<[&'static str]>,
    /// Fixed parameter metadata in PHP call order.
    pub(in crate::interpreter) params: Box<[EvalParamSpec]>,
    /// Variadic parameter name, when supported.
    pub(in crate::interpreter) variadic: Option<&'static str>,
    /// Parameter names that must bind by reference.
    pub(in crate::interpreter) by_ref_params: Box<[&'static str]>,
    /// Explicit required parameter count for non-trailing default shapes.
    pub(in crate::interpreter) required_param_count: Option<usize>,
    /// Direct expression-level dispatch hook.
    pub(in crate::interpreter) direct: Option<EvalDirectHook>,
    /// Evaluated-argument dispatch hook.
    pub(in crate::interpreter) values: Option<EvalValuesHook>,
    /// Workspace-relative path of the home file that declared this builtin.
    pub(in crate::interpreter) home_file: &'static str,
    /// Typed generated-runtime implementation, when boxed-cell semantics agree.
    pub(in crate::interpreter) runtime_builtin: Option<RuntimeBuiltinId>,
    /// Machine-audited shared-runtime or documented adapter route.
    pub(in crate::interpreter) execution: EvalExecution,
    /// Whether strict-PHP hides this elephc-only surface.
    extension: bool,
}

impl EvalBuiltinSpec {
    /// Joins one Magician implementation binding to its validated shared contract.
    pub(in crate::interpreter) fn from_binding(binding: &'static EvalBuiltinBinding) -> Self {
        let contract = lookup_id(binding.id).unwrap_or_else(|| {
            panic!(
                "eval builtin binding references unknown shared contract ID {} from {}",
                binding.id.as_u64(),
                binding.home_file
            )
        });
        let signature = eval_signature(contract);
        let params = signature
            .params
            .iter()
            .copied()
            .map(eval_param_spec)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let mut param_names = signature
            .params
            .iter()
            .map(|param| param.name)
            .collect::<Vec<_>>();
        if let Some(variadic) = signature.variadic {
            param_names.push(variadic);
        }
        let by_ref_params = signature
            .params
            .iter()
            .filter(|param| param.by_ref)
            .map(|param| param.name)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let execution = eval_execution(contract)
            .expect("eval builtin binding must reference an eval-supported shared contract");

        Self {
            name: contract.name,
            area: binding.area,
            param_names: param_names.into_boxed_slice(),
            params,
            variadic: signature.variadic,
            by_ref_params,
            required_param_count: signature.required_param_count,
            direct: binding.direct,
            values: binding.values,
            home_file: binding.home_file,
            runtime_builtin: execution.runtime_builtin(),
            execution,
            extension: contract.extension,
        }
    }

    /// Returns this builtin's file-layout area.
    pub(in crate::interpreter) fn area(&self) -> EvalArea {
        self.area
    }

    /// Returns whether strict-PHP hides this elephc-only builtin.
    pub(in crate::interpreter) fn is_extension(&self) -> bool {
        self.extension
    }

    /// Returns the number of required leading parameters.
    pub(in crate::interpreter) fn required_param_count(&self) -> usize {
        self.required_param_count.unwrap_or_else(|| {
            self.params
                .iter()
                .take_while(|param| param.default.is_none())
                .count()
        })
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

    /// Returns by-reference parameter names from the shared signature profile.
    pub(in crate::interpreter) fn by_ref_param_names(&self) -> &[&'static str] {
        self.by_ref_params.as_ref()
    }

    /// Returns the default value for one PHP parameter slot.
    pub(in crate::interpreter) fn default_value(
        &self,
        param_index: usize,
    ) -> Option<EvalBuiltinDefaultValue> {
        self.params.get(param_index).and_then(|param| param.default)
    }
}

/// Converts one neutral parameter contract into Magician's runtime default representation.
fn eval_param_spec(param: ParamSpec) -> EvalParamSpec {
    EvalParamSpec {
        name: param.name,
        default: param.default.map(eval_default_value),
        by_ref: param.by_ref,
    }
}

/// Converts one neutral PHP default into the runtime materialization enum.
fn eval_default_value(default: DefaultSpec) -> EvalBuiltinDefaultValue {
    match default {
        DefaultSpec::Null => EvalBuiltinDefaultValue::Null,
        DefaultSpec::Int(value) => EvalBuiltinDefaultValue::Int(value),
        DefaultSpec::Bool(value) => EvalBuiltinDefaultValue::Bool(value),
        DefaultSpec::Float(value) => EvalBuiltinDefaultValue::Float(value),
        DefaultSpec::Str(value) => EvalBuiltinDefaultValue::String(value),
        DefaultSpec::IntMax => EvalBuiltinDefaultValue::Int(i64::MAX),
        DefaultSpec::EmptyArray => EvalBuiltinDefaultValue::EmptyArray,
    }
}

inventory::collect!(EvalBuiltinBinding);
