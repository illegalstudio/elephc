//! Purpose:
//! Defines the canonical set of PHP builtin functions known to the type system.
//! Provides case-insensitive lookup used by name resolution, redeclaration checks, and PHP visibility checks.
//!
//! Called from:
//! - `crate::types::checker::builtins`
//! - `crate::name_resolver`
//!
//! Key details:
//! - `SUPPORTED_BUILTIN_FUNCTIONS` is the source of truth for PHP-visible builtin names.
//! - `INTERNAL_BUILTIN_FUNCTIONS` is now an empty placeholder; internal builtins are
//!   registered via `internal: true` in `src/builtins/` and recognized through the registry.

const SUPPORTED_BUILTIN_FUNCTIONS: &[&str] = &[
    "boolval",
    "buffer_free",
    "buffer_len",
    "buffer_new",
    "call_user_func",
    "call_user_func_array",
    "die",
    "empty",
    "exit",
    "floatval",
    "class_alias",
    "class_exists",
    "class_implements",
    "class_parents",
    "class_uses",
    "enum_exists",
    "function_exists",
    "get_class",
    "get_parent_class",
    "get_resource_id",
    "get_resource_type",
    "get_declared_classes",
    "get_declared_interfaces",
    "get_declared_traits",
    "interface_exists",
    "trait_exists",
    "gettype",
    "intval",
    "is_bool",
    "is_callable",
    "is_a",
    "is_array",
    "is_object",
    "is_scalar",
    "is_finite",
    "is_float",
    "is_subclass_of",
    "is_infinite",
    "is_int",
    "is_iterable",
    "is_nan",
    "is_null",
    "is_numeric",
    "is_resource",
    "is_string",
    "isset",
    "preg_replace_callback",

    "settype",
    "strlen",
    "stream_isatty",
    "stream_socket_server",
    "stream_socket_client",
    "stream_socket_accept",
    "fsockopen",
    "pfsockopen",
    "stream_socket_enable_crypto",
    "stream_resolve_include_path",
    "stream_set_chunk_size",
    "stream_set_read_buffer",
    "stream_set_write_buffer",
    "stream_get_contents",
    "stream_get_line",
    "stream_get_meta_data",
    "stream_set_blocking",
    "stream_set_timeout",
    "stream_select",
    "stream_socket_shutdown",
    "stream_socket_sendto",
    "stream_socket_recvfrom",
    "stream_socket_get_name",
    "stream_socket_pair",
    "gethostname",
    "gethostbyname",
    "gethostbyaddr",
    "getprotobyname",
    "getprotobynumber",
    "getservbyname",
    "getservbyport",
    "stream_copy_to_stream",
    "stream_is_local",
    "stream_supports_lock",
    "stream_get_transports",
    "stream_get_wrappers",
    "stream_get_filters",
    "unset",
];

// All former entries migrated to `src/builtins/io/__elephc_phar_*.rs` with `internal: true`
// (io batch C2). Name recognition now flows through `registry::is_supported` inside
// `canonical_builtin_function_name`. The slice is kept as an empty placeholder so that
// `is_supported_builtin_function_exact` compiles unchanged.
const INTERNAL_BUILTIN_FUNCTIONS: &[&str] = &[];

/// Checks if the exact (lowercase) name is in the PHP-visible or internal builtin lists.
/// Does not perform case folding; use `is_supported_builtin_function` for case-insensitive lookup.
fn is_supported_builtin_function_exact(name: &str) -> bool {
    SUPPORTED_BUILTIN_FUNCTIONS.contains(&name) || INTERNAL_BUILTIN_FUNCTIONS.contains(&name)
}

/// Returns the union of PHP-visible supported builtin function names from the
/// legacy static list and the builtin registry.
///
/// Registry entries flagged as `internal` are excluded, mirroring the semantics
/// of `is_php_visible_builtin_function`. Names present in both sources appear
/// exactly once. With an empty registry this returns the legacy list unchanged,
/// so behavior is preserved while the registry is empty.
pub(crate) fn supported_builtin_function_names() -> Vec<&'static str> {
    let mut result: Vec<&'static str> = SUPPORTED_BUILTIN_FUNCTIONS.to_vec();
    for name in crate::builtins::registry::names() {
        let def = match crate::builtins::registry::lookup(name) {
            Some(d) => d,
            None => continue,
        };
        if def.spec.internal {
            continue;
        }
        // De-duplicate: skip names already present in the legacy list.
        let lower = name.to_ascii_lowercase();
        if !SUPPORTED_BUILTIN_FUNCTIONS.contains(&lower.as_str()) {
            result.push(def.name);
        }
    }
    result
}

/// Converts a function name to lowercase and returns it if it is a supported builtin.
///
/// Returns `None` if the name is not in either the legacy catalog or the builtin
/// registry. Implements PHP's case-insensitive builtin lookup. The legacy static
/// list is consulted first; the registry is the fallback.
pub(crate) fn canonical_builtin_function_name(name: &str) -> Option<String> {
    let canonical = name.to_ascii_lowercase();
    if is_supported_builtin_function_exact(&canonical)
        || crate::builtins::registry::is_supported(&canonical)
    {
        Some(canonical)
    } else {
        None
    }
}

/// Returns true only for PHP-visible builtin functions (non-internal builtins).
///
/// Checks both the legacy static list and the builtin registry. Registry entries
/// flagged as `internal` are excluded from the PHP-visible set.
pub(crate) fn is_php_visible_builtin_function(name: &str) -> bool {
    let canonical = name.to_ascii_lowercase();
    SUPPORTED_BUILTIN_FUNCTIONS.contains(&canonical.as_str())
        || crate::builtins::registry::lookup(&canonical)
            .map(|def| !def.spec.internal)
            .unwrap_or(false)
}

/// Returns `true` if the name is a supported builtin function (case-insensitive).
/// Delegates to `canonical_builtin_function_name` and checks for `Some`.
pub(crate) fn is_supported_builtin_function(name: &str) -> bool {
    canonical_builtin_function_name(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin;

    /// No-op lowering hook for test probe; does nothing and succeeds.
    fn noop_lower(
        _c: &mut crate::codegen_ir::context::FunctionContext,
        _i: &crate::ir::Instruction,
    ) -> Result<(), crate::codegen_ir::CodegenIrError> {
        Ok(())
    }

    // Register a PHP-visible (non-internal) probe to exercise the catalog API.
    // This verifies that `supported_builtin_function_names` and the catalog
    // lookup functions include registry entries with `internal: false`.
    builtin! {
        name: "__catalog_probe_visible",
        area: Internal,
        params: [x: Int],
        returns: Bool,
        lower: noop_lower,
        summary: "catalog probe for PHP-visibility test",
        internal: false,
    }

    /// Verifies that a `builtin!`-registered probe with `internal: false` is reported
    /// as supported by the catalog's `is_supported_builtin_function` and
    /// `canonical_builtin_function_name` surfaces.
    #[test]
    fn catalog_reports_registered_visible_probe_as_supported() {
        assert!(
            is_supported_builtin_function("__catalog_probe_visible"),
            "catalog must report a non-internal registered builtin as supported"
        );
        let canonical = canonical_builtin_function_name("__catalog_probe_visible");
        assert_eq!(
            canonical,
            Some("__catalog_probe_visible".to_string()),
            "catalog must canonicalize a non-internal registered builtin"
        );
    }

    /// Verifies that a non-internal registered probe appears in `supported_builtin_function_names`.
    #[test]
    fn supported_builtin_function_names_includes_registered_visible_probe() {
        let names = supported_builtin_function_names();
        assert!(
            names.contains(&"__catalog_probe_visible"),
            "supported_builtin_function_names must include non-internal registry entries"
        );
    }
}
