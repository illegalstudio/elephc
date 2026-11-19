//! Purpose:
//! PHP 8.6's flush/close failure propagation for `fclose()`, `file_put_contents()` and `copy()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests stream_close_failure_php86`.
//!
//! Key details:
//! - PHP 8.6 makes `fclose()` return false when the stream's flush fails, so a wrapper whose
//!   `stream_flush()` returns false reports a failed close. PHP 8.5 still returns true.
//! - `copy()` must accept an empty source (a zero-byte write is a success), which the ARM64
//!   result predicate used to reject.
//! - The filesystem fixtures run in the per-test temporary directory, like the other io tests.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// A failing `stream_flush()` turns `fclose()` into false on 8.6 and stays true on 8.5.
#[test]
fn test_fclose_reports_a_failed_stream_flush_php86() {
    let source = r#"<?php
class FailingFlushWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_write($data): int { return 0; }
    public function stream_flush(): bool { return false; }
    public function stream_close(): void {}
}
stream_wrapper_register('failflush', 'FailingFlushWrapper');
$handle = fopen('failflush://value', 'w');
var_dump(fclose($handle));
"#;
    let out = compile_and_run_with_php_version(source, PhpVersion::Php86);
    assert_eq!(out, "bool(false)\n");

    let older = compile_and_run_with_php_version(source, PhpVersion::Php85);
    assert_eq!(older, "bool(true)\n");
}

/// A successful flush keeps `fclose()` true, and an absent `stream_flush()` is not a failure.
#[test]
fn test_fclose_stays_true_without_a_failing_flush() {
    let source = r#"<?php
class SilentFlushWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_write($data): int { return 0; }
    public function stream_flush(): bool { return true; }
    public function stream_close(): void {}
}
class NoFlushWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_write($data): int { return 0; }
    public function stream_close(): void {}
}
stream_wrapper_register('silentflush', 'SilentFlushWrapper');
stream_wrapper_register('noflush', 'NoFlushWrapper');
var_dump(fclose(fopen('silentflush://value', 'w')));
var_dump(fclose(fopen('noflush://value', 'w')));
"#;
    let out = compile_and_run_with_php_version(source, PhpVersion::Php86);
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// `copy()` accepts an empty source and still rejects a missing one.
#[test]
fn test_copy_accepts_an_empty_source() {
    let out = compile_and_run(
        r#"<?php
file_put_contents('empty-copy-source.txt', '');
var_dump(copy('empty-copy-source.txt', 'empty-copy-target.txt'));
var_dump(file_get_contents('empty-copy-target.txt'));
var_dump(copy('missing-copy-source.txt', 'missing-copy-target.txt'));
"#,
    );
    assert_eq!(out, "bool(true)\nstring(0) \"\"\nbool(false)\n");
}

/// `file_put_contents()` still reports the byte count of a successful write.
#[test]
fn test_file_put_contents_keeps_the_byte_count() {
    let out = compile_and_run(
        r#"<?php
var_dump(file_put_contents('fpc-byte-count.txt', 'abc'));
var_dump(file_get_contents('fpc-byte-count.txt'));
"#,
    );
    assert_eq!(out, "int(3)\nstring(3) \"abc\"\n");
}
