//! Purpose:
//! Reject closed boxed streams before native descriptor operations.
//!
//! Called from:
//! - `cargo test --test codegen_tests closed_stream_resources`.
//!
//! Key details:
//! - Explicit close is visible through aliases, independently of resource refcounts.
//! - Closed-resource TypeErrors are catchable and exception cleanup remains balanced.

use crate::support::*;

/// Runs one boxed-stream diagnostic fixture and requires its expected output and clean heap.
fn assert_closed_stream(body: &str, expected: &str) {
    let source = format!(
        "<?php\nfunction boxed(mixed $value): mixed {{ return $value; }}\n{body}\n"
    );
    let out = compile_and_run_with_heap_debug(&source);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Makes explicit close visible to every boxed stream alias before a later write.
#[test]
fn closed_stream_resources_write_through_alias_throws_type_error() {
    assert_closed_stream(
        r#"
$source = boxed(fopen('/dev/null', 'w'));
$alias = boxed($source);
echo fwrite($alias, 'x'), '|';
fclose($alias);
try {
    fwrite($source, 'again');
    echo 'accepted';
} catch (TypeError $error) {
    echo $error->getMessage();
    unset($error);
}
unset($source); unset($alias);
"#,
        "1|fwrite(): Argument #1 ($stream) must be an open stream resource",
    );
}

/// Rejects reads from a closed boxed stream with balanced exception cleanup.
#[test]
fn closed_stream_resources_read_throws_type_error() {
    assert_closed_stream(
        r#"
$source = boxed(fopen('/dev/null', 'r'));
fclose($source);
try {
    fread($source, 1);
    echo 'accepted';
} catch (TypeError $error) {
    echo $error->getMessage();
    unset($error);
}
unset($source);
"#,
        "fread(): Argument #1 ($stream) must be an open stream resource",
    );
}

/// Reports a second close without invoking native close on the already-retired descriptor.
#[test]
fn closed_stream_resources_second_close_throws_without_reclosing_descriptor() {
    assert_closed_stream(
        r#"
$source = boxed(fopen('/dev/null', 'w'));
fclose($source);
try {
    fclose($source);
    echo 'accepted';
} catch (TypeError $error) {
    echo $error->getMessage();
    unset($error);
}
unset($source);
"#,
        "fclose(): Argument #1 ($stream) must be an open stream resource",
    );
}
