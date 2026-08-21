//! Purpose:
//! Home of the internal `__elephc_curl_easy_setopt_long` builtin: applies an
//! integer-valued `curl_setopt()` option to a raw easy handle.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - PHP's `curl_setopt()` takes `mixed $value` and picks the libcurl setter from the
//!   value's runtime type. That dispatch lives in the PRELUDE, not here: PHP's own type
//!   juggling (`bool`/`int`/`float` all become a `long`) is exactly the kind of logic
//!   that belongs in elephc-PHP, and splitting the raw entry points by argument type
//!   keeps each lowering a fixed-shape C call instead of a tag-inspecting one.
//! - Returns whether the option was accepted. `CURLOPT_RETURNTRANSFER` is a PHP-only
//!   pseudo-option handled inside the bridge (`crates/elephc-curl/src/php_layer.rs`) and
//!   never reaches libcurl; every other option is forwarded unchanged, so a value
//!   libcurl rejects answers `false` rather than an inert `true`.

builtin! {
    contract: "__elephc_curl_easy_setopt_long",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasySetoptLong,
    ),
}
