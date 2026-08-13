//! Purpose:
//! Home of the internal `__elephc_curl_share_init_persistent` builtin: builds (or finds)
//! the process-lifetime share behind PHP 8.5's `curl_share_init_persistent()`.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_share_init_persistent()` in `crate::curl_prelude` (the
//!   PHP >= 8.5 fenced block).
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THE ARGUMENT IS A COMMA-SEPARATED STRING OF DECIMAL `CURL_LOCK_DATA_*` INTS, not a
//!   PHP array: this crate's C ABI has no native array shape, and `curl_setopt()`'s own
//!   string-list option (`CURLOPT_HTTPHEADER`, ...) already establishes the pattern of
//!   encoding a variable-length PHP value as one byte blob for a fixed-arity C entry point
//!   — CSV rather than that option's NUL-framing because every element here is a plain
//!   non-negative int with no embedded-NUL hazard to guard against. The PRELUDE validates
//!   every element is one of the five real `CURL_LOCK_DATA_*` values PHP exposes BEFORE
//!   encoding it this way (`ValueError` otherwise, per php-src) — this builtin's own
//!   lowering never needs to invent a new marshalling shape for it.
//! - OWNERSHIP: like `__elephc_curl_share_init`, this boxes a resource-kind-8 Mixed cell.
//!   Its release path is a documented NO-OP (`crates/elephc-curl/src/share.rs`'s
//!   `elephc_curl_share_free`): the underlying share lives until the process exits, which
//!   is elephc's answer to php-src's PHP-FPM-worker-scoped persistence (elephc has no such
//!   worker-restart boundary to key a shorter lifetime off).
//! - `false` ON ALLOCATION FAILURE, the same divergence `__elephc_curl_share_init`
//!   documents, turned into a thrown `RuntimeException` by the prelude.

builtin! {
    name: "__elephc_curl_share_init_persistent",
    area: Curl,
    params: [lock_data_csv: Str],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlShareInitPersistent,
    ),
    summary: "Builds or finds the process-lifetime share for curl_share_init_persistent() (PHP 8.5).",
    internal: true,
}
