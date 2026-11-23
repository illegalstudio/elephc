//! Purpose:
//! Runs PHP 8.6's `CURLINFO_SIZE_DELIVERED` / `size_delivered` fixture.
//!
//! Called from:
//! - `cargo test --test codegen_tests size_delivered_php86`.
//!
//! Key details:
//! - PHP 8.6 adds the `size_delivered` key to `curl_getinfo()`'s array and the
//!   `CURLINFO_SIZE_DELIVERED` option form, both reporting the bytes passed to
//!   the download write callback. Requires the managed libcurl 8.20+.
//! - The loopback fixture serves `hello-curl` (10 bytes).

use super::http_fixture::LocalHttpServer;
use crate::support::*;
use elephc::php_version::PhpVersion;

/// Verifies both info forms report the delivered byte count on 8.6.
#[test]
fn test_curl_size_delivered_php86() {
    if skip_without_curl_native("curl_size_delivered_php86") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run_with_php_version(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $body = curl_exec($ch);
        $info = curl_getinfo($ch);
        echo $info["size_delivered"], "|";
        echo curl_getinfo($ch, CURLINFO_SIZE_DELIVERED), "|";
        echo strlen($body);
        "#
    ), PhpVersion::Php86);
    assert_eq!(out, "10|10|10");
}
