//! Purpose:
//! Public builtin metadata snapshots used by parity tests and external audits.
//! Keeps catalog and call-signature details observable without duplicating
//! builtin lists outside the compiler-owned sources of truth.
//!
//! Called from:
//! - Rust integration tests that compare `elephc` and `elephc-magician` builtin support.
//!
//! Key details:
//! - Names come from the checker builtin catalog.
//! - Signature shapes are derived from `FunctionSig`, not maintained by hand.

/// A compact, comparison-friendly view of a builtin call signature.
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

/// Returns the compiler's PHP-visible builtin names.
///
/// Reads the unfiltered catalog snapshot — never the strict-PHP-filtered view —
/// so the memoized result is independent of the thread's strict-mode state.
pub fn php_visible_builtin_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<&'static [&'static str]> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let names =
            crate::types::checker::builtins::catalog::all_supported_builtin_function_names();
        Box::leak(names.into_boxed_slice())
    })
}

/// Returns the compiler's PHP-visible extension builtin names (elephc-only
/// builtins hidden by `--strict-php`), in stable sorted order. Reads the
/// registry's `extension` flags directly — never the strict-filtered catalog —
/// so the snapshot is independent of the thread's strict-mode state. Includes
/// the catalog-name-only `buffer_new` entry.
pub fn extension_builtin_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| {
            let mut names: Vec<&'static str> = vec!["buffer_new"];
            for name in crate::builtins::registry::names() {
                let Some(def) = crate::builtins::registry::lookup(name) else {
                    continue;
                };
                if def.spec.extension && !def.spec.internal {
                    names.push(def.name);
                }
            }
            names.sort_unstable();
            names
        })
        .as_slice()
}

/// Returns comparison metadata for one builtin signature, when the compiler tracks it.
pub fn builtin_signature_metadata(name: &str) -> Option<BuiltinSignatureMetadata> {
    let canonical = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let sig = crate::types::builtin_call_sig(&canonical)?;
    let params = sig
        .params
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let required_param_count = sig
        .defaults
        .iter()
        .position(Option::is_some)
        .unwrap_or(sig.params.len());
    let default_param_count = sig.defaults.iter().filter(|default| default.is_some()).count();
    let by_ref_params = sig
        .params
        .iter()
        .zip(sig.ref_params.iter())
        .filter_map(|((name, _), is_ref)| is_ref.then(|| name.clone()))
        .collect::<Vec<_>>();

    Some(BuiltinSignatureMetadata {
        params,
        required_param_count,
        default_param_count,
        variadic: sig.variadic,
        by_ref_params,
    })
}
