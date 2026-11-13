//! Purpose:
//! Checks temporary filename and data owners at the file_put_contents boundary.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Narrowed Mixed parameters and fresh string expressions exercise both input leases.
//! - Raw byte-count results cannot share their storage with either string.

use crate::support::*;

/// File writes retire independently owned input strings after ordinary and appended writes.
#[test]
fn test_file_write_results_release_temporary_inputs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function writeNarrowedFileOwners(mixed $filename, mixed $content): void {
    $filename = __DIR__ . '/file-write-owner-narrowed';
    $content = str_repeat('x', 9);
    echo file_put_contents($filename, $content), ':';
    $filename = null;
    $content = null;
}
for ($index = 0; $index < 3; $index++) { writeNarrowedFileOwners(null, null); }
$repeat = (int)($argc > 0);
echo file_put_contents(str_repeat(__DIR__ . '/file-write-owner-fresh', $repeat), str_repeat('y', $argc + 3)), ':';
echo file_put_contents(str_repeat(__DIR__ . '/file-write-owner-fresh', $repeat), str_repeat('z', $argc + 2), FILE_APPEND), ':';
echo file_get_contents(__DIR__ . '/file-write-owner-fresh');
unlink(__DIR__ . '/file-write-owner-narrowed');
unlink(__DIR__ . '/file-write-owner-fresh');
"#);
    assert!(out.success, "stdout={:?}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "9:9:9:4:3:yyyyzzz", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// File input retirement also follows the shared contract without EIR optimization.
#[test]
fn test_file_write_results_release_temporary_inputs_no_ir_opt() {
    without_ir_opt(test_file_write_results_release_temporary_inputs);
}
