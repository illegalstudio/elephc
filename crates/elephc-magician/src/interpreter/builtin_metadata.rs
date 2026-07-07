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
    eval_declared_builtin_exists,
    eval_builtin_param_names, eval_builtin_signature_shape, eval_php_visible_builtin_exists,
    eval_php_visible_builtin_function_names,
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

/// Normalizes a PHP symbol name for case-insensitive builtin lookup.
fn php_symbol_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}
