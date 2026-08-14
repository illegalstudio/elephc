//! Purpose:
//! End-to-end proof that `ext/curl` integer constants are real, stable PHP values.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - `CURLOPT_URL` is one of the four frozen constant prefixes
//!   (`src/curl_prelude/detect.rs`), so merely mentioning it still injects the curl
//!   prelude and links the `elephc_curl` bridge, which in turn needs the managed native
//!   `curl`/`openssl`/`zlib` archives — hence the same `skip_without_curl_native` gate the
//!   rest of `tests/codegen/curl/` uses. That linkage is a side effect of *detection*, not
//!   of the constant itself: the constant's VALUE is a compile-time literal from
//!   `crate::types::curl_constants::CURL_INT_CONSTANTS`, resolved without ever touching
//!   libcurl at runtime (see the module doc comment on `src/types/curl_constants.rs`).

use crate::support::*;

/// `CURLOPT_URL` resolves to its frozen libcurl value (10002) and stays that value across
/// compilations — the constant is a baked literal, not something read from the linked
/// libcurl at runtime.
#[test]
fn curlopt_url_is_defined_and_stable() {
    if skip_without_curl_native("curlopt_url_is_defined_and_stable") {
        return;
    }
    let out = compile_and_run("<?php echo CURLOPT_URL;");
    let expected = 10002;
    assert_eq!(out, expected.to_string());
}
