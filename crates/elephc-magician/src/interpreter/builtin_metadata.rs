//! Purpose:
//! Public metadata view for eval-interpreter builtin support.
//! Gives parity tests a stable API for builtin existence and named-argument
//! parameter lists without duplicating the interpreter registry.
//!
//! Called from:
//! - `elephc-magician::builtin_metadata` re-export.
//!
//! Key details:
//! - Lookup normalizes names with PHP-style case-insensitivity.
//! - Signature shape is the same registry data used by eval named-argument binding.

use super::builtins::{
    eval_builtin_param_names, eval_builtin_signature_shape, eval_date_procedural_alias_names,
    eval_declared_builtin_exists, eval_declared_builtin_spec, eval_php_visible_builtin_exists,
    eval_php_visible_builtin_function_names, EvalBuiltinDefaultValue,
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

/// Returns whether the eval interpreter exposes a PHP-visible builtin name.
pub fn php_visible_builtin_exists(name: &str) -> bool {
    let canonical = php_symbol_key(name);
    eval_php_visible_builtin_exists(&canonical)
}

/// Returns the eval interpreter's PHP-visible builtin names.
pub fn php_visible_builtin_names() -> &'static [&'static str] {
    eval_php_visible_builtin_function_names()
}

/// Returns whether the eval builtin is backed by the declarative registry.
pub fn php_visible_builtin_is_registry_declared(name: &str) -> bool {
    let canonical = php_symbol_key(name);
    eval_declared_builtin_exists(&canonical)
}

/// Returns comparison metadata for one eval builtin signature, when named calls are tracked.
pub fn builtin_signature_metadata(name: &str) -> Option<BuiltinSignatureMetadata> {
    let canonical = php_symbol_key(name);
    let params = eval_builtin_param_names(&canonical)?
        .iter()
        .map(|param| (*param).to_string())
        .collect::<Vec<_>>();
    let shape = eval_builtin_signature_shape(&canonical)?;
    Some(BuiltinSignatureMetadata {
        params,
        required_param_count: shape.required_param_count,
        default_param_count: shape.default_param_count,
        variadic: shape.variadic.map(str::to_string),
        by_ref_params: shape
            .by_ref_params
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
    /// Workspace-relative home file that declared the builtin.
    pub home_file: String,
}

/// Returns documentation metadata for one registry-declared eval builtin.
pub fn builtin_docs_metadata(name: &str) -> Option<BuiltinDocsMetadata> {
    let canonical = php_symbol_key(name);
    let spec = eval_declared_builtin_spec(&canonical)?;
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
        home_file: spec.home_file.to_string(),
    })
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
        EvalBuiltinDefaultValue::Bytes(value) => {
            format!("\"{}\"", String::from_utf8_lossy(value).escape_default())
        }
        EvalBuiltinDefaultValue::EmptyArray => "[]".to_string(),
    }
}

/// Normalizes a PHP symbol name for case-insensitive builtin lookup.
fn php_symbol_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}
