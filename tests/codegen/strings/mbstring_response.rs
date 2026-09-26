//! Purpose:
//! Verifies native header validation and terminal commitment for programs using shared mbstring.
//!
//! Called from:
//! - The focused codegen string integration harness.
//!
//! Key details:
//! - Buffered and return-mode output must not commit response metadata prematurely.
//! - Header warnings obey runtime suppression and leave native heap ownership balanced.

use crate::support::*;

/// Keeps headers mutable during buffered/return-mode output and rejects them after the terminal flush.
#[test]
fn test_mbstring_response_buffered_headers() {
    let output = compile_and_run_with_heap_debug(r#"<?php
mb_internal_encoding("UTF-8");
$rendered = print_r([1, 2], true);
header("Content-Type: text/plain");
ob_start();
echo "buffered";
header("X-Before-Flush: accepted");
ob_end_flush();
@header("X-Suppressed: ignored");
header("X-Late: ignored");
echo ":done";
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "buffered:done");
    assert_eq!(output.stderr.matches("Warning: header(): Cannot modify header information - headers already sent").count(), 1, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Rejects malformed header bytes before output while preserving ordinary subsequent header calls.
#[test]
fn test_mbstring_response_header_validation() {
    let output = compile_and_run_with_heap_debug(r#"<?php
mb_http_output("ISO-8859-1");
header("Content-Type: bad\0value");
header("Content-Type: text/plain\nX: ignored");
header("Content-Type: text/plain\r\n");
echo "ready";
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "ready");
    assert!(output.stderr.contains("Warning: header(): Header may not contain NUL bytes"), "{}", output.stderr);
    assert!(output.stderr.contains("Warning: header(): Header may not contain more than a single header, new line detected"), "{}", output.stderr);
    assert!(!output.stderr.contains("headers already sent"), "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
