//! Purpose:
//! Compile-time diagnostic tests for the `ext/curl` prelude surface: `CurlHandle` is
//! `final` and not user-constructible, and the `curl_*` wrappers reject wrong argument
//! counts and wrong argument types.
//!
//! Called from:
//! - `cargo test --test error_tests curl` through Rust's test harness.
//!
//! Key details:
//! - These need only the INJECTED PRELUDE, never a link, so unlike
//!   `tests/codegen/curl/` they run on every machine whether or not the managed native
//!   `curl` package is installed. That is exactly why the two object-model rules
//!   (`final`, private constructor) are pinned here rather than there.
//! - `check_source` already injects the curl prelude between alias collection and name
//!   resolution, mirroring `pipeline::compile`, so the checker sees the prelude's typed
//!   signatures.
//! - Every assertion also checks the message is a REAL diagnostic rather than an
//!   "Undefined function/class" one, which is what a silently broken injection would
//!   produce.

use super::*;

/// Asserts `src` fails to compile and the message contains `needle`, with a guard against
/// a missing-prelude error masquerading as the expected diagnostic.
fn expect_curl_error(src: &str, needle: &str) {
    let error = check_source(src).expect_err("program must fail to compile");
    assert!(
        !error.contains("Undefined function") && !error.contains("Undefined class"),
        "curl prelude was not injected; got: {error}"
    );
    assert!(
        error.contains(needle),
        "expected an error containing {needle:?}, got: {error}"
    );
}

/// PHP's `CurlHandle` is `final`: a session object is minted by `curl_init()` and by
/// nothing else, so a user class cannot extend it.
#[test]
fn curl_handle_cannot_be_extended() {
    expect_curl_error(
        "<?php class MyHandle extends CurlHandle {} $h = curl_init();",
        "final",
    );
}

/// `CurlHandle`'s constructor is private, matching php-src, so `new CurlHandle()` is
/// rejected. A user must go through `curl_init()`.
#[test]
fn curl_handle_cannot_be_constructed() {
    expect_curl_error("<?php $h = new CurlHandle();", "private");
}

/// `curl_setopt()` takes exactly three arguments, so a call missing the value is an arity
/// error rather than a silently ignored option.
#[test]
fn curl_setopt_rejects_wrong_arity() {
    expect_curl_error(
        "<?php $ch = curl_init(); curl_setopt($ch, 10002);",
        "curl_setopt",
    );
}

/// A `curl_*` function that takes a handle rejects a non-handle argument at compile time,
/// which is the earliest point elephc can tell the difference.
#[test]
fn curl_exec_rejects_a_non_handle() {
    expect_curl_error(r#"<?php curl_exec("not a handle");"#, "curl_exec");
}

/// `curl_init()` accepts an optional URL string; passing an int is a type error.
#[test]
fn curl_init_rejects_a_non_string_url() {
    expect_curl_error("<?php $ch = curl_init(42);", "curl_init");
}
