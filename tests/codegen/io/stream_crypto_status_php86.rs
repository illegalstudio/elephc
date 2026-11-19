//! Purpose:
//! PHP 8.6's `stream_socket_get_crypto_status()` and the `STREAM_CRYPTO_STATUS_*` constants.
//!
//! Called from:
//! - `cargo test --test codegen_tests stream_crypto_status_php86`.
//!
//! Key details:
//! - elephc completes TLS handshakes synchronously, so an attached session reports
//!   `STREAM_CRYPTO_STATUS_NONE`; a stream without an SSL layer warns like php-src and also
//!   reports NONE. The warning is asserted through the merged stream and suppressed with `@`.
//! - A user-wrapper stream uses a synthetic descriptor above the bounded TLS session table, so
//!   it must take the warning path instead of indexing the table.

use crate::support::*;

/// The three status constants carry php-src's values.
#[test]
fn test_stream_crypto_status_constants() {
    let out = compile_and_run(
        r#"<?php
var_dump(STREAM_CRYPTO_STATUS_NONE);
var_dump(STREAM_CRYPTO_STATUS_WANT_READ);
var_dump(STREAM_CRYPTO_STATUS_WANT_WRITE);
"#,
    );
    assert_eq!(out, "int(0)\nint(1)\nint(2)\n");
}

/// A plain socket pair has no pending crypto operation: NONE plus php-src's warning.
#[test]
fn test_stream_crypto_status_plain_socket_warns_and_reports_none() {
    let out = compile_and_run_capture_merged(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
var_dump(stream_socket_get_crypto_status($pair[0]) === STREAM_CRYPTO_STATUS_NONE);
fclose($pair[0]);
fclose($pair[1]);
"#,
    )
    .stdout;
    assert!(
        out.contains("This stream does not support SSL/crypto"),
        "{out}"
    );
    assert!(out.contains("bool(true)"), "{out}");
}

/// `@` suppresses the unsupported-stream warning while the result stays NONE.
#[test]
fn test_stream_crypto_status_suppression_keeps_none() {
    let out = compile_and_run_capture_merged(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
var_dump(@stream_socket_get_crypto_status($pair[0]));
fclose($pair[0]);
fclose($pair[1]);
"#,
    )
    .stdout;
    assert!(
        !out.contains("does not support SSL/crypto"),
        "the `@` operator must suppress the warning: {out}"
    );
    assert!(out.contains("int(0)"), "{out}");
}

/// A user-wrapper stream sits above the TLS session table and still reports NONE.
#[test]
fn test_stream_crypto_status_user_wrapper_reports_none() {
    let out = compile_and_run_capture_merged(
        r#"<?php
class CryptoStatusWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_write($data): int { return 0; }
    public function stream_close(): void {}
}
stream_wrapper_register('cryptostatus', 'CryptoStatusWrapper');
$handle = fopen('cryptostatus://value', 'w');
var_dump(@stream_socket_get_crypto_status($handle));
fclose($handle);
"#,
    )
    .stdout;
    assert!(out.contains("int(0)"), "{out}");
}
