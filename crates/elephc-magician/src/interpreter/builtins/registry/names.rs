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
/// module doc, "Scope shipped vs. deferred"). Checked BEFORE every native-function
/// fallback that could otherwise reach one of these names — `crate::interpreter::
/// expressions::eval_call`, the `call_user_func()`/`call_user_func_array()` string-callback
/// paths (`registry::callable::object_dispatch`), and the plain-callable-array-argument
/// paths (`registry::callable::array_dispatch`, which also cover a bare `$f()` variable
/// call and `ReflectionFunction::invoke()`) — because whenever the compiled program
/// hosting this `eval()` call also links `elephc_curl` (i.e. it uses curl anywhere outside
/// `eval()` too, which is what makes `elephc_curl` available at all), the AOT prelude
/// registers a REAL `curl_multi_init()`/`curl_share_init()`/`curl_file_create()` symbol in
/// `ElephcEvalContext::native_function()`. Without this check, calling one of those names
/// from inside `eval()` — through ANY of those paths — would silently run the COMPILED
/// implementation and hand back a real AOT `CurlMultiHandle`/`CurlShareHandle` object —
/// indistinguishable from a genuinely supported call until it is mixed with an eval-owned
/// easy handle and fails confusingly (that module doc's "TWO DISTINCT OBJECT SPACES"
/// section). This check turns that silent, confusing interop failure into the same
/// "eval() fragment uses an unsupported construct" fatal any other undefined-in-eval name
/// already produces.
///
/// GATED behind this crate's own `curl` Cargo feature, and that gate is not a
/// simplification — it is exactly as precise as the bug it closes. The build that can
/// actually observe `native_function("curl_multi_init")` resolving to the real extension
/// function is, by construction, the SAME build this guard needs to exist in:
/// `src/linker/bridges.rs`'s bridge-archive resolver selects/builds the curl-aware
/// `libelephc_magician_curl.a` (this crate compiled `--features curl`) exactly when a
/// program needs BOTH `elephc_magician` (calls `eval()`) AND `elephc_curl` together — the
/// identical `needs_curl` condition that also decides whether `elephc_curl` itself gets
/// linked at all. A build of this crate WITHOUT the `curl` feature can never be paired
/// with a real `elephc_curl` link, so `native_function()` can never resolve a genuine curl
/// symbol there — and gating this way closes a real false-positive the unconditional
/// version had: outside a curl-linked program, nothing stops a user script from declaring
/// its OWN `function curl_multi_init() { ... }` (PHP only forbids redeclaring a name once
/// the REAL extension function already exists, which requires curl to be loaded), and an
/// unconditional prefix match would have rejected that legitimate call too. Inside a
/// curl-linked program, no such collision is possible — PHP does not allow redeclaring a
/// function the loaded `curl` extension already defines — so the guard is exactly as wide
/// as it needs to be and no wider.
///
/// Matching is by PREFIX (`curl_multi_`/`curl_share_`) rather than an exhaustive name
/// list, so PHP 8.5 additions (`curl_multi_get_handles`, `curl_share_init_persistent`,
/// …) are covered without needing a matching update here.
#[cfg(feature = "curl")]
pub(in crate::interpreter) fn eval_curl_deferred_function_name(name: &str) -> bool {
    name.starts_with("curl_multi_") || name.starts_with("curl_share_") || name == "curl_file_create"
}

/// Returns true for `CURLFile`/`CURLStringFile` — the two classes
/// `curl_file_create()`/`CURLOPT_POSTFIELDS`'s multipart array form need, which this eval
/// interpreter deliberately never implements (same module doc as
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
