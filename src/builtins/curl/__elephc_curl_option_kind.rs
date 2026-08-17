//! Purpose:
//! Home of the internal `__elephc_curl_option_kind` builtin: classifies a `curl_setopt()`
//! option number so the prelude knows which setter (if any) can carry its value.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - THIS IS THE GATE `curl_setopt()` IS BUILT AROUND. `curl_easy_setopt` is variadic and
//!   reads its third argument according to the option's numeric range, so handing a PHP
//!   value to the wrong range is a wild pointer, not a wrong answer. The frozen table in
//!   `crates/elephc-curl/src/options.rs` is what decides, and this builtin is how the
//!   prelude reaches it.
//! - THE ANSWER IS A SMALL INTEGER KIND CODE, not a boolean: `0` invalid (php-src's
//!   `ValueError`), `1` long, `2` string, `3` slist, `4` off_t, `5` PHP-layer pseudo-option,
//!   `6` recognized but not carryable by this build (`false` + PHP's warning). The prelude
//!   branches on it; the codes are defined once, in the bridge's `options` module.
//! - IT TAKES NO HANDLE, so it is the only curl builtin whose lowering marshals nothing
//!   but a plain integer. It still declares the bridge requirement, because the table it
//!   consults lives in the bridge.

builtin! {
    contract: "__elephc_curl_option_kind",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlOptionKind,
    ),
}
