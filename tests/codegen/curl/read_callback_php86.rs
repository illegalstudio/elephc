//! Purpose:
//! Runs PHP 8.6's CURLOPT_READFUNCTION return-value validation fixture.
//!
//! Called from:
//! - `cargo test --test codegen_tests read_callback_php86`.
//!
//! Key details:
//! - PHP 8.6 rejects a read callback returning an integer other than 0,
//!   CURL_READFUNC_ABORT or CURL_READFUNC_PAUSE with a ValueError.
//! - The upload runs against the loopback `/echo` route, which drains the body.

use elephc::php_version::PhpVersion;

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with the PHP 8.5 profile.
fn run85(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php85)
}

/// Verifies the 8.6 ValueError for an invalid integer return and the 8.5 silence.
#[test]
fn test_curl_read_callback_invalid_int_php86() {
    if skip_without_curl_native("curl_read_callback_invalid_int_php86") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let source = format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        curl_setopt($ch, CURLOPT_INFILESIZE, 4);
        curl_setopt($ch, CURLOPT_READFUNCTION, function ($ch, $fd, $len) {{ return 42; }});
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        try {{
            curl_exec($ch);
            echo "ok";
        }} catch (ValueError $e) {{
            echo $e->getMessage();
        }}
        "#
    );
    assert_eq!(
        run86(&source),
        "The CURLOPT_READFUNCTION callback must return a string or CURL_READFUNC_ABORT or CURL_READFUNC_PAUSE"
    );
    assert_eq!(run85(&source), "ok", "PHP 8.5 coerces the integer return to end-of-data");
}

/// Verifies the new abort constant resolves to its pinned libcurl value.
#[test]
fn test_curl_readfunc_abort_constant_php86() {
    let out = compile_and_run_with_php_version(
        "<?php echo CURL_READFUNC_ABORT, '|', CURL_READFUNC_PAUSE;",
        PhpVersion::Php86,
    );
    assert_eq!(out, "268435456|268435457");
}
