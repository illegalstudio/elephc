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

/// Returns true for a `curl_multi_*`/`curl_share_*`/`curl_file_create` name — the curl
/// multi interface, share interface, and `CURLFile` factory function this eval
/// interpreter deliberately never implements (`crate::interpreter::builtins::curl`'s
/// module doc, "Scope shipped vs. deferred"). `crate::interpreter::expressions::eval_call`
/// checks this BEFORE its native-function fallback: whenever the compiled program hosting
/// this `eval()` call also links `elephc_curl` (i.e. it uses curl anywhere outside
/// `eval()` too, which is what makes `elephc_curl` available at all), the AOT prelude
/// registers a REAL `curl_multi_init()`/`curl_share_init()`/`curl_file_create()` symbol in
/// `ElephcEvalContext::native_function()`. Without this check, calling one of those names
/// from inside `eval()` would silently run the COMPILED implementation and hand back a
/// real AOT `CurlMultiHandle`/`CurlShareHandle` object — indistinguishable from a
/// genuinely supported call until it is mixed with an eval-owned easy handle and fails
/// confusingly (that module doc's "TWO DISTINCT OBJECT SPACES" section). This check turns
/// that silent, confusing interop failure into the same "eval() fragment uses an
/// unsupported construct" fatal any other undefined-in-eval name already produces.
///
/// Unconditional (not gated behind this crate's own `curl` Cargo feature): whether
/// `native_function()` can actually resolve one of these names depends on what the FINAL
/// compiled PROGRAM links, not on how `elephc-magician` itself was built, so the guard has
/// to exist even in a build of this crate that never compiles
/// `crate::interpreter::builtins::curl` at all.
///
/// Matching is by PREFIX (`curl_multi_`/`curl_share_`) rather than an exhaustive name
/// list, so PHP 8.5 additions (`curl_multi_get_handles`, `curl_share_init_persistent`,
/// …) are covered without needing a matching update here.
pub(in crate::interpreter) fn eval_curl_deferred_function_name(name: &str) -> bool {
    name.starts_with("curl_multi_") || name.starts_with("curl_share_") || name == "curl_file_create"
}

/// Returns true for `CURLFile`/`CURLStringFile` — the two classes
/// `curl_file_create()`/`CURLOPT_POSTFIELDS`'s multipart array form need, which this eval
/// interpreter deliberately never implements (same module doc as
/// `eval_curl_deferred_function_name`). `crate::interpreter::expressions::evaluation::
/// eval_new_object_result` checks this before its native-class fallback for the exact same
/// reason: `new CURLFile(...)` inside `eval()` would otherwise silently construct a real
/// AOT `CURLFile` object whenever the compiled program links curl (verified: it does,
/// before this check existed).
///
/// Case-insensitive, matching PHP's own class-name comparison rules.
pub(in crate::interpreter) fn eval_curl_deferred_class_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("CURLFile") || name.eq_ignore_ascii_case("CURLStringFile")
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
