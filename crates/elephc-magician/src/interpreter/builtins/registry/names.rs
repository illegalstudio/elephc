//! Purpose:
//! Builtin existence helpers used by eval function probes.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Declarative specs are the source of truth for PHP-visible eval builtin names.
//! - Lookup callers pass canonical lowercase PHP symbol names.

use super::{
    eval_declared_builtin_exists, eval_declared_builtin_function_names,
    eval_raw_declared_builtin_spec,
};

/// Returns the eval interpreter's PHP-visible builtin names.
pub(in crate::interpreter) fn eval_php_visible_builtin_function_names() -> &'static [&'static str] {
    eval_declared_builtin_function_names()
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
pub(in crate::interpreter) fn eval_php_visible_builtin_exists(name: &str) -> bool {
    eval_declared_builtin_exists(name)
}

/// Returns true for a builtin this eval interpreter implements but the COMPILED PHP
/// COMPATIBILITY PROFILE does not have, so INTROSPECTION must not report it.
///
/// Only the two PHP 8.5 curl additions land here today. The AOT side gets this for free
/// from the curl prelude's `-- elephc PHP >= 8.5 ... --` source fences, which
/// `prelude_source_for_version` strips below 8.5: the declaration is simply absent, so
/// `function_exists('curl_multi_get_handles')` answers `false` and calling it is
/// "Call to undefined function". eval carries ONE registry for every profile, so both halves
/// have to be reproduced by hand — and they are reproduced in two DIFFERENT places on
/// purpose:
///
/// - INTROSPECTION consults this predicate, which is what makes it agree with AOT.
///   `function_exists()` is the whole of that surface today: eval has no PHP-visible
///   `get_defined_functions()`, and `php_visible_builtin_names()` is test-harness
///   introspection rather than anything a PHP program can observe.
/// - DISPATCH deliberately does NOT. Hiding the name from dispatch too would send the call
///   down the "no builtin, no user function, no native function" path, whose answer is
///   eval's UNCATCHABLE "unsupported construct" fatal — strictly worse than AOT's catchable
///   `Error`. The call instead reaches the builtin's own home, where
///   `eval_curl_require_php_85` raises exactly the `Error: Call to undefined function
///   curl_multi_get_handles()` a real 8.4 runtime raises.
///
/// Gated behind the `curl` feature because its whole content is: a curl-free build has no
/// name to hide.
#[cfg(feature = "curl")]
pub(in crate::interpreter) fn eval_builtin_hidden_by_php_version(name: &str) -> bool {
    matches!(name, "curl_multi_get_handles" | "curl_share_init_persistent")
        && crate::eval_php_profile::eval_php_version_id() < 80_500
}

/// The curl-free build's stand-in: nothing is version-hidden, because the only names that
/// ever are live behind the `curl` feature.
#[cfg(not(feature = "curl"))]
pub(in crate::interpreter) fn eval_builtin_hidden_by_php_version(_name: &str) -> bool {
    false
}

/// Returns the eval builtins that are elephc extensions (no PHP equivalent),
/// in stable sorted order. Strict-PHP binaries hide exactly this set from eval
/// dispatch and introspection. Derived from the RAW registry so the snapshot
/// is independent of the thread's strict-mode state.
pub(in crate::interpreter) fn eval_extension_builtin_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| {
            eval_declared_builtin_function_names()
                .iter()
                .copied()
                .filter(|name| {
                    eval_raw_declared_builtin_spec(name)
                        .is_some_and(|spec| spec.is_extension())
                })
                .collect()
        })
        .as_slice()
}
