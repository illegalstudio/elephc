//! Purpose:
//! Public raw-catalog metadata view for eval-interpreter builtin support.
//! Gives parity tests and docs a stable API for builtin existence and
//! named-argument parameter lists without duplicating the registry.
//!
//! Called from:
//! - `elephc-magician::builtin_metadata` re-export.
//!
//! Key details:
//! - Lookup normalizes names with PHP-style case-insensitivity.
//! - Runtime capability filters do not remove entries from this metadata view.
//! - Signature shape is the same registry data used by eval named-argument binding.

use super::builtins::{
    eval_date_procedural_alias_names, eval_extension_builtin_names,
    eval_php_visible_builtin_function_names, eval_raw_declared_builtin_spec,
    EvalBuiltinDefaultValue,
};
use elephc_builtin_contract::{
    eval_signature_profile, lookup, EvalAdapterReason, EvalExecution,
    EvalSignatureOverrideReason,
};

/// A compact, comparison-friendly view of an eval builtin call signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinSignatureMetadata {
    /// Parameter names in PHP call order.
    pub params: Vec<String>,
    /// Number of leading parameters that must be supplied positionally or by name.
    pub required_param_count: usize,
    /// Number of parameters that carry explicit default values.
    pub default_param_count: usize,
    /// Name of the variadic parameter, when the builtin accepts one.
    pub variadic: Option<String>,
    /// Parameter names that must be passed by reference.
    pub by_ref_params: Vec<String>,
}

/// Returns whether the eval interpreter catalog implements a PHP-visible builtin name.
pub fn php_visible_builtin_exists(name: &str) -> bool {
    let canonical = php_symbol_key(name);
    eval_raw_declared_builtin_spec(&canonical).is_some()
}

/// Returns the eval interpreter's PHP-visible builtin names.
pub fn php_visible_builtin_names() -> &'static [&'static str] {
    eval_php_visible_builtin_function_names()
}

/// Returns whether the eval builtin is backed by the declarative registry.
pub fn php_visible_builtin_is_registry_declared(name: &str) -> bool {
    let canonical = php_symbol_key(name);
    eval_raw_declared_builtin_spec(&canonical).is_some()
}

/// Returns the eval builtins that are elephc extensions (no PHP equivalent),
/// in stable sorted order. Strict-PHP binaries hide exactly this set from eval
/// dispatch and introspection; parity gates compare it against the compiler
/// registry's `extension`-flagged builtins.
pub fn extension_builtin_names() -> &'static [&'static str] {
    eval_extension_builtin_names()
}

/// Returns comparison metadata for one eval builtin signature, when named calls are tracked.
pub fn builtin_signature_metadata(name: &str) -> Option<BuiltinSignatureMetadata> {
    let canonical = php_symbol_key(name);
    let spec = eval_raw_declared_builtin_spec(&canonical)?;
    let params = spec
        .param_names
        .iter()
        .map(|param| (*param).to_string())
        .collect::<Vec<_>>();
    Some(BuiltinSignatureMetadata {
        params,
        required_param_count: spec.required_param_count(),
        default_param_count: spec.default_param_count(),
        variadic: spec.variadic.map(str::to_string),
        by_ref_params: spec
            .by_ref_param_names()
            .iter()
            .map(|param| (*param).to_string())
            .collect(),
    })
}

/// Parameter metadata for one eval builtin, documentation-oriented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinDocsParam {
    /// PHP-visible parameter name.
    pub name: String,
    /// Whether the parameter binds caller storage by reference.
    pub by_ref: bool,
    /// PHP-source spelling of the default value, when the parameter is optional.
    pub default: Option<String>,
}

/// Documentation-oriented metadata for one registry-declared eval builtin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinDocsMetadata {
    /// Canonical registry name.
    pub name: String,
    /// Lowercase eval-area spelling (interpreter file layout family).
    pub area: String,
    /// Parameters in PHP call order.
    pub params: Vec<BuiltinDocsParam>,
    /// Variadic parameter name, when the builtin accepts one.
    pub variadic: Option<String>,
    /// Number of leading parameters that must be supplied.
    pub required_param_count: usize,
    /// Whether an expression-level dispatch hook is registered.
    pub has_direct_hook: bool,
    /// Whether an evaluated-argument dispatch hook is registered.
    pub has_values_hook: bool,
    /// Stable boxed-runtime ABI ID used by this binding, when any.
    pub runtime_builtin_id: Option<u32>,
    /// `shared-runtime`, `hybrid-adapter`, or `interpreter-adapter`.
    pub execution: &'static str,
    /// Machine-readable reason a Magician adapter remains.
    pub adapter_reason: Option<&'static str>,
    /// Machine-readable reason eval's signature differs from the canonical contract.
    pub signature_override_reason: Option<&'static str>,
    /// Workspace-relative home file that declared the builtin.
    pub home_file: String,
}

/// Returns documentation metadata for one registry-declared eval builtin.
pub fn builtin_docs_metadata(name: &str) -> Option<BuiltinDocsMetadata> {
    let canonical = php_symbol_key(name);
    let spec = eval_raw_declared_builtin_spec(&canonical)?;
    let (execution, adapter_reason) = eval_execution_metadata(spec.execution);
    let signature_override_reason = eval_signature_profile(
        lookup(spec.name).expect("eval metadata must resolve its shared contract"),
    )
    .override_reason
    .map(eval_signature_override_reason_name);
    Some(BuiltinDocsMetadata {
        name: spec.name.to_string(),
        area: spec.area().name().to_string(),
        params: spec
            .params
            .iter()
            .map(|param| BuiltinDocsParam {
                name: param.name.to_string(),
                by_ref: param.by_ref,
                default: param.default.map(default_value_php_repr),
            })
            .collect(),
        variadic: spec.variadic.map(str::to_string),
        required_param_count: spec.required_param_count(),
        has_direct_hook: spec.direct.is_some(),
        has_values_hook: spec.values.is_some(),
        runtime_builtin_id: spec.runtime_builtin.map(|id| id.as_u32()),
        execution,
        adapter_reason,
        signature_override_reason,
        home_file: spec.home_file.to_string(),
    })
}

/// Renders one typed eval execution route for generated documentation metadata.
fn eval_execution_metadata(
    execution: EvalExecution,
) -> (&'static str, Option<&'static str>) {
    match execution {
        EvalExecution::SharedRuntime(_) => ("shared-runtime", None),
        EvalExecution::Adapter {
            runtime_builtin: Some(_),
            reason,
        } => ("hybrid-adapter", Some(eval_adapter_reason_name(reason))),
        EvalExecution::Adapter {
            runtime_builtin: None,
            reason,
        } => ("interpreter-adapter", Some(eval_adapter_reason_name(reason))),
    }
}

/// Returns the stable documentation spelling for one adapter reason.
fn eval_adapter_reason_name(reason: EvalAdapterReason) -> &'static str {
    match reason {
        EvalAdapterReason::ByReferenceOrLvalue => "by-reference-or-lvalue",
        EvalAdapterReason::CallableOrReflection => "callable-or-reflection",
        EvalAdapterReason::DynamicObjectCoercion => "dynamic-object-coercion",
        EvalAdapterReason::DynamicLanguageSurface => "dynamic-language-surface",
        EvalAdapterReason::CapabilityDependent => "capability-dependent",
        EvalAdapterReason::RuntimeStateOrResource => "runtime-state-or-resource",
        EvalAdapterReason::InterpreterSpecificValueSemantics => {
            "interpreter-specific-value-semantics"
        }
        EvalAdapterReason::AdditionalSignatureSemantics => "additional-signature-semantics",
    }
}

/// Returns the stable documentation spelling for one eval signature override reason.
fn eval_signature_override_reason_name(reason: EvalSignatureOverrideReason) -> &'static str {
    match reason {
        EvalSignatureOverrideReason::NonTrailingRequiredParameter => {
            "non-trailing-required-parameter"
        }
        EvalSignatureOverrideReason::RuntimeDefaultRepresentation => {
            "runtime-default-representation"
        }
        EvalSignatureOverrideReason::AdditionalOptionalParameters => {
            "additional-optional-parameters"
        }
        EvalSignatureOverrideReason::AdditionalByReferenceOutput => {
            "additional-by-reference-output"
        }
    }
}

/// Returns the procedural date/time alias names the eval dispatcher accepts
/// without declarative registry entries.
pub fn date_procedural_alias_names() -> &'static [&'static str] {
    eval_date_procedural_alias_names()
}

/// Formats an eval builtin default value with its PHP-source spelling.
fn default_value_php_repr(value: EvalBuiltinDefaultValue) -> String {
    match value {
        EvalBuiltinDefaultValue::Null => "null".to_string(),
        EvalBuiltinDefaultValue::Bool(true) => "true".to_string(),
        EvalBuiltinDefaultValue::Bool(false) => "false".to_string(),
        EvalBuiltinDefaultValue::Int(value) => value.to_string(),
        EvalBuiltinDefaultValue::Float(value) => format!("{:?}", value),
        EvalBuiltinDefaultValue::String(value) => format!("\"{}\"", value.escape_default()),
        EvalBuiltinDefaultValue::EmptyArray => "[]".to_string(),
        EvalBuiltinDefaultValue::ClassConstant { class, name } => {
            format!("{class}::{name}")
        }
    }
}

/// Normalizes a PHP symbol name for case-insensitive builtin lookup.
fn php_symbol_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}
