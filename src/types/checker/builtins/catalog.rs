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
//! - `LANGUAGE_CONSTRUCT_FUNCTIONS` participates in call resolution but stays
//!   hidden from `function_exists()` and first-class callable surfaces.

const SUPPORTED_BUILTIN_FUNCTIONS: &[&str] = &[
    // `buffer_new` is a catalog-name-only entry: `buffer_new<T>(len)` is parsed as
    // dedicated syntax (`ExprKind::BufferNew`), so the name never dispatches as a
    // builtin call; it is listed here for `function_exists`, case-insensitive
    // lookup, and redeclaration checks. `buffer_len`/`buffer_free` live in the
    // registry (`src/builtins/pointers/`). Like them, it is an elephc extension
    // hidden by `--strict-php`.
    "buffer_new",
    "die",
    "empty",
    "exit",
    "is_double",
    "is_integer",
    "is_long",
    "is_real",
    "isset",
    "method_exists",
    "property_exists",
    "strval",
    "unset",
];

// All former entries migrated to `src/builtins/io/__elephc_phar_*.rs` with `internal: true`
// (io batch C2). Name recognition now flows through `registry::is_supported` inside
// `canonical_builtin_function_name`. The slice is kept as an empty placeholder so that
// `is_supported_builtin_function_exact` compiles unchanged.
const INTERNAL_BUILTIN_FUNCTIONS: &[&str] = &[];

const LANGUAGE_CONSTRUCT_FUNCTIONS: &[&str] = &["eval"];

/// Checks if the exact (lowercase) name is in any callable-resolution builtin list.
/// Does not perform case folding; use `is_supported_builtin_function` for case-insensitive lookup.
fn is_supported_builtin_function_exact(name: &str) -> bool {
    SUPPORTED_BUILTIN_FUNCTIONS.contains(&name)
        || INTERNAL_BUILTIN_FUNCTIONS.contains(&name)
        || LANGUAGE_CONSTRUCT_FUNCTIONS.contains(&name)
}

/// Returns true when `--strict-php` hides the (lowercase) name from user programs.
///
/// Extension builtins have no PHP equivalent, so strict mode makes them behave
/// as if they did not exist: calls fall through to user-function resolution and
/// the standard undefined-function diagnostics, redeclaration checks accept user
/// functions with these names, and `function_exists()` reports `false`.
/// `internal: true` builtins are never hidden — injected compiler preludes call
/// them and they are already invisible to user programs. `buffer_new` is the one
/// catalog-name-only extension (its call form is dedicated syntax).
pub(crate) fn strict_php_hidden_builtin(canonical: &str) -> bool {
    if !crate::strict_php::is_enabled() {
        return false;
    }
    if canonical == "buffer_new" {
        return true;
    }
    crate::builtins::registry::lookup(canonical)
        .map(|def| def.spec.extension && !def.spec.internal)
        .unwrap_or(false)
}

/// Returns the union of PHP-visible builtin names from the legacy static list
/// and the builtin registry, WITHOUT the strict-PHP filter.
///
/// This is the raw catalog snapshot for metadata consumers (parity gates, docs
/// exporters) that memoize the result and must be independent of the thread's
/// strict-mode state. Compilation surfaces use `supported_builtin_function_names`.
pub(crate) fn all_supported_builtin_function_names() -> Vec<&'static str> {
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

/// Returns the union of PHP-visible supported builtin function names from the
/// legacy static list and the builtin registry.
///
/// Registry entries flagged as `internal` are excluded, mirroring the semantics
/// of `is_php_visible_builtin_function`. Names present in both sources appear
/// exactly once. With an empty registry this returns the legacy list unchanged,
/// so behavior is preserved while the registry is empty. Under `--strict-php`,
/// extension builtins are excluded entirely.
pub(crate) fn supported_builtin_function_names() -> Vec<&'static str> {
    all_supported_builtin_function_names()
        .into_iter()
        .filter(|name| !strict_php_hidden_builtin(&name.to_ascii_lowercase()))
        .collect()
}

/// Converts a function name to lowercase and returns it if it is a supported builtin.
///
/// Returns `None` if the name is not in either the legacy catalog or the builtin
/// registry, or if `--strict-php` hides it (extension builtins). Implements PHP's
/// case-insensitive builtin lookup. The legacy static list is consulted first;
/// the registry is the fallback.
pub(crate) fn canonical_builtin_function_name(name: &str) -> Option<String> {
    let canonical = name.to_ascii_lowercase();
    if strict_php_hidden_builtin(&canonical) {
        return None;
    }
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
/// flagged as `internal` are excluded from the PHP-visible set, and `--strict-php`
/// additionally excludes extension builtins.
pub(crate) fn is_php_visible_builtin_function(name: &str) -> bool {
    let canonical = name.to_ascii_lowercase();
    if strict_php_hidden_builtin(&canonical) {
        return false;
    }
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
        _c: &mut crate::codegen::context::FunctionContext,
        _i: &crate::ir::Instruction,
    ) -> Result<(), crate::codegen::CodegenIrError> {
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

    /// Verifies strict mode hides extension builtins from every catalog surface:
    /// canonical lookup, PHP-visibility, the supported-name set, and the
    /// `buffer_new` catalog-name-only entry. Strict state is thread-local (and
    /// guard-restored on panic), so this cannot affect parallel tests.
    #[test]
    fn strict_mode_hides_extension_builtins_from_catalog() {
        let _guard = crate::strict_php::scoped_enable();
        assert!(
            canonical_builtin_function_name("ptr_get").is_none(),
            "strict must hide ptr_get"
        );
        assert!(
            !is_php_visible_builtin_function("ptr_get"),
            "strict must hide ptr_get from PHP visibility"
        );
        assert!(
            canonical_builtin_function_name("buffer_new").is_none(),
            "strict must hide buffer_new"
        );
        assert!(
            !is_php_visible_builtin_function("buffer_new"),
            "strict must hide buffer_new from PHP visibility"
        );
        let names = supported_builtin_function_names();
        assert!(
            !names.contains(&"ptr_get") && !names.contains(&"buffer_new"),
            "strict must drop extension names from the supported set"
        );
    }

    /// Verifies strict mode keeps genuine PHP builtins and internal prelude
    /// aliases resolvable: hiding either would break normal programs or
    /// compiler-injected prelude code.
    #[test]
    fn strict_mode_keeps_php_builtins_and_internal_aliases() {
        let _guard = crate::strict_php::scoped_enable();
        assert_eq!(
            canonical_builtin_function_name("strlen"),
            Some("strlen".to_string())
        );
        assert_eq!(
            canonical_builtin_function_name("is_real"),
            Some("is_real".to_string()),
            "is_real is treated as PHP for strict purposes"
        );
        assert!(
            canonical_builtin_function_name("__elephc_ptr_read_string").is_some(),
            "internal prelude aliases must stay resolvable in strict mode"
        );
    }

    /// Verifies the unfiltered name set ignores strict mode entirely: metadata
    /// consumers (parity gates, docs exporters) memoize this snapshot and must
    /// never observe a strict-filtered view.
    #[test]
    fn unfiltered_name_set_ignores_strict_mode() {
        let _guard = crate::strict_php::scoped_enable();
        let names = all_supported_builtin_function_names();
        assert!(names.contains(&"ptr_get"));
        assert!(names.contains(&"buffer_new"));
        assert!(names.contains(&"strlen"));
    }

    /// Verifies extension builtins remain fully visible without strict mode, so
    /// the filter cannot regress the default compilation mode.
    #[test]
    fn non_strict_keeps_extension_builtins_visible() {
        assert!(canonical_builtin_function_name("ptr_get").is_some());
        assert!(is_php_visible_builtin_function("ptr_get"));
        assert!(canonical_builtin_function_name("buffer_new").is_some());
        assert!(supported_builtin_function_names().contains(&"buffer_new"));
    }
}
