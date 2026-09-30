//! Purpose:
//! TDD regressions for stream-filter chain ordering, registry capacity, and lifecycle.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_stream_filter_registry_` once this module is wired.
//!
//! Key details:
//! - Expected output is pinned to the local PHP 8.5.6 oracle.
//! - Built-in string filters isolate chain bookkeeping from user-filter dispatch where possible.
//! - User-filter cases use the canonical brigade API and observe `onClose` through static counters.

use crate::support::*;

/// Verifies three read and three write filters honor append and prepend ordering.
#[test]
fn test_stream_filter_registry_orders_append_and_prepend_chains() {
    let out = compile_and_run(
        r#"<?php
$read = fopen("php://memory", "r+");
fwrite($read, "AbC");
rewind($read);
stream_filter_append($read, "string.tolower", STREAM_FILTER_READ);
stream_filter_append($read, "string.rot13", STREAM_FILTER_READ);
stream_filter_prepend($read, "string.toupper", STREAM_FILTER_READ);
echo stream_get_contents($read);
fclose($read);

echo "|";

$write = fopen("php://memory", "r+");
stream_filter_append($write, "string.tolower", STREAM_FILTER_WRITE);
stream_filter_append($write, "string.rot13", STREAM_FILTER_WRITE);
stream_filter_prepend($write, "string.toupper", STREAM_FILTER_WRITE);
fwrite($write, "AbC");
rewind($write);
echo stream_get_contents($write), "\n";
fclose($write);
"#,
    );
    assert_eq!(out, "nop|nop\n");
}

/// Verifies a payload larger than 64 KiB is transformed completely without truncation.
#[test]
fn test_stream_filter_registry_filters_payload_larger_than_64_kib() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://memory", "r+");
stream_filter_append($stream, "string.toupper", STREAM_FILTER_WRITE);
$input = str_repeat("aBc", 30000);
$written = fwrite($stream, $input);
rewind($stream);
$output = stream_get_contents($stream);
echo $written, "|", strlen($output), "|";
echo $output === str_repeat("ABC", 30000) ? "ok\n" : "bad\n";
fclose($stream);
"#,
    );
    assert_eq!(out, "90000|90000|ok\n");
}

/// Verifies removing a middle filter leaves both neighboring nodes attached.
#[test]
fn test_stream_filter_registry_remove_middle_preserves_neighbors() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://memory", "r+");
stream_filter_append($stream, "string.tolower", STREAM_FILTER_WRITE);
$middle = stream_filter_append($stream, "string.rot13", STREAM_FILTER_WRITE);
stream_filter_append($stream, "string.toupper", STREAM_FILTER_WRITE);
var_dump(stream_filter_remove($middle));
fwrite($stream, "AbC");
rewind($stream);
echo stream_get_contents($stream), "\n";
fclose($stream);
"#,
    );
    assert_eq!(out, "bool(true)\nABC\n");
}

/// Verifies `STREAM_FILTER_ALL` returns a resource handle that removal invalidates.
#[test]
fn test_stream_filter_registry_all_mode_returns_lifecycle_handle() {
    let out = compile_and_run(
        r#"<?php
$stream = fopen("php://memory", "r+");
fwrite($stream, "abc");
rewind($stream);
$filter = stream_filter_append($stream, "string.rot13", STREAM_FILTER_ALL);
echo is_resource($filter) ? "1" : "0";
echo "|", stream_get_contents($stream), "|";
echo stream_filter_remove($filter) ? "1" : "0";
echo "|", is_resource($filter) ? "1" : "0", "\n";
fclose($stream);
"#,
    );
    assert_eq!(out, "1|nop|1|0\n");
}

/// Verifies more than 256 filtered streams retain isolated chains and close their handles.
#[test]
fn test_stream_filter_registry_supports_more_than_256_isolated_streams() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$streams = [];
$filters = [];
for ($i = 0; $i < 300; $i++) {
    $stream = fopen("php://memory", "r+");
    $filter = stream_filter_append(
        $stream,
        $i % 2 === 0 ? "string.toupper" : "string.rot13",
        STREAM_FILTER_WRITE
    );
    fwrite($stream, "a" . $i);
    $streams[] = $stream;
    $filters[] = $filter;
}

$errors = 0;
for ($i = 0; $i < 300; $i++) {
    rewind($streams[$i]);
    $actual = stream_get_contents($streams[$i]);
    $expected = ($i % 2 === 0 ? "A" : "n") . $i;
    if ($actual !== $expected) {
        $errors++;
    }
}

foreach ($streams as $stream) {
    fclose($stream);
}
$closedHandles = 0;
foreach ($filters as $filter) {
    if (!is_resource($filter)) {
        $closedHandles++;
    }
}
echo count($streams), " ", $errors, " ", $closedHandles, "\n";
"#,
        64 * 1024 * 1024,
    );
    assert_eq!(out, "300 0 300\n");
}

/// Verifies stream close calls a user filter's `onClose` once and invalidates all handle aliases.
#[test]
fn test_stream_filter_registry_stream_close_invalidates_handle_and_calls_onclose_once() {
    let out = compile_and_run(
        r#"<?php
class CloseCountingFilter extends php_user_filter {
    public static $closed = 0;

    public function filter($in, $out, &$consumed, $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }

    public function onClose(): void {
        self::$closed = self::$closed + 1;
    }
}

stream_filter_register("close.counting", "CloseCountingFilter");
$stream = fopen("php://memory", "r+");
$filter = stream_filter_append($stream, "close.counting", STREAM_FILTER_WRITE);
$aliases = [$filter];
fwrite($stream, "x");
fclose($stream);
echo CloseCountingFilter::$closed, "|";
echo is_resource($filter) ? "open" : "closed";
echo "|", is_resource($aliases[0]) ? "open" : "closed", "\n";
"#,
    );
    assert_eq!(out, "1|closed|closed\n");
}

/// Verifies a failed closing flush leaves the filter attached for later writes.
#[test]
fn test_stream_filter_registry_failed_flush_preserves_filter_node() {
    let out = compile_and_run(
        r#"<?php
class FailFlushFilter extends php_user_filter {
    public static $closed = 0;

    public function filter($in, $out, &$consumed, $closing): int {
        if ($closing) {
            return PSFS_ERR_FATAL;
        }
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            $bucket->datalen = strlen($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }

    public function onClose(): void {
        self::$closed = self::$closed + 1;
    }
}

stream_filter_register("fail.flush", "FailFlushFilter");
$stream = fopen("php://memory", "r+");
$filter = stream_filter_append($stream, "fail.flush", STREAM_FILTER_WRITE);
fwrite($stream, "x");
var_dump(stream_filter_remove($filter));
var_dump(is_resource($filter));
fwrite($stream, "y");
rewind($stream);
echo stream_get_contents($stream), "|", FailFlushFilter::$closed, "\n";
fclose($stream);
echo FailFlushFilter::$closed, "\n";
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nXY|0\n1\n");
}
