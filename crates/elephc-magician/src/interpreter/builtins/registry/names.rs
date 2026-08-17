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

/// Returns true for `curl_file_create` — the ONE curl name this eval interpreter still
/// routes away from `ElephcEvalContext::native_function()`.
///
/// HISTORY, because the shape of this guard is not obvious: it used to match the whole
/// `curl_multi_*`/`curl_share_*` PREFIX space too, back when the eval curl surface was
/// easy-interface-only. Whenever the compiled program hosting an `eval()` call also links
/// `elephc_curl` (i.e. it uses curl anywhere outside `eval()`, which is what makes the
/// bridge available at all), the AOT prelude registers REAL `curl_multi_init()`/
/// `curl_share_init()`/`curl_file_create()` symbols in `native_function()`; without a
/// guard, calling one from inside `eval()` silently ran the COMPILED implementation and
/// handed back a genuine AOT `CurlMultiHandle`/`CurlShareHandle` — indistinguishable from
/// a supported call right up until it was mixed with an eval-owned easy handle and failed
/// confusingly (`crate::interpreter::builtins::curl`'s "TWO DISTINCT OBJECT SPACES"). The
/// multi and share interfaces now have real eval homes
/// (`crate::interpreter::builtins::curl::curl_multi_init` and friends), so those names are
/// answered by the eval builtin registry BEFORE any native fallback is consulted and need
/// no interception at all.
///
/// `curl_file_create()` IS STILL LISTED because `CURLFile`/`CURLStringFile` are still
/// rejected as classes (see `eval_curl_deferred_class_name` below): a factory that can only
/// hand back a class this interpreter refuses to construct would be a worse failure than an
/// honest rejection.
///
/// GATED behind this crate's own `curl` Cargo feature, and that gate is not a
/// simplification — it is exactly as precise as the bug it closes. The build that can
/// actually observe `native_function("curl_file_create")` resolving to the real extension
/// function is, by construction, the SAME build this guard needs to exist in:
/// `src/linker/bridges.rs`'s bridge-archive resolver selects/builds the curl-aware
/// `libelephc_magician_curl.a` (this crate compiled `--features curl`) exactly when a
/// program needs BOTH `elephc_magician` (calls `eval()`) AND `elephc_curl` together — the
/// identical `needs_curl` condition that also decides whether `elephc_curl` itself gets
/// linked at all. A build of this crate WITHOUT the `curl` feature can never be paired
/// with a real `elephc_curl` link, so `native_function()` can never resolve a genuine curl
/// symbol there.
#[cfg(feature = "curl")]
pub(in crate::interpreter) fn eval_curl_deferred_function_name(name: &str) -> bool {
    name == "curl_file_create"
}

/// Returns true for `CURLFile`/`CURLStringFile` — the two classes
/// `curl_file_create()`/`CURLOPT_POSTFIELDS`'s multipart array form need, which this eval
/// interpreter does not yet construct (same module doc as
/// `eval_curl_deferred_function_name`, whose "gated behind `curl`" rationale applies here
/// identically). Checked before every native-class-construction fallback that could
/// otherwise reach one of these names — `crate::interpreter::expressions::evaluation::
/// eval_new_object_result` (plain `new CURLFile(...)`) and the `ReflectionClass`/
/// `ReflectionAttribute` instantiation paths (`statements::reflection_instantiation`) —
/// because `new CURLFile(...)` inside `eval()` would otherwise silently construct a real
/// AOT `CURLFile` object whenever the compiled program links curl (verified: it did,
/// before this check existed).
///
/// Case-insensitive, matching PHP's own class-name comparison rules.
#[cfg(feature = "curl")]
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
