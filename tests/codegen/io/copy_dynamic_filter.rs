//! Purpose:
//! Integration tests for `copy()` whose SOURCE is spelled at run time: a `php://filter/...` URL
//! assembled from variables must read through the filter chain, as `fopen()` already does.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `__rt_copy` reads through `__rt_file_get_contents`, whose runtime half is a `stat` and an
//!   `open(2)`. It reaches every REGISTERED wrapper and knows nothing of php's own filter scheme,
//!   which only the lowering resolves — so the very URL `fopen()` and `file_get_contents()` both
//!   open answered `Failed to open stream` here. The literal spelling worked, which is what made
//!   it look like a filter problem rather than a SPELLING one.
//! - Only the FILTER route is emitted for a run-time source. The whole dynamic reader
//!   `file_get_contents()` uses pulls zlib into the link of every program that calls `copy()`,
//!   and its other arms answered a copy with a crash — measured, both.
//! - Everything a run-time path did before must still hold: the ordinary copy, the self-copy
//!   php refuses by `(st_dev, st_ino)`, the missing source, and the empty source that copies
//!   successfully. They are asserted here because they now travel through a new branch.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies a `php://filter/...` source assembled at run time reads through the chain.
#[test]
fn test_a_run_time_filter_url_is_copied_through_its_chain() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("raw.txt", "uryyb");
$src = "php://filter/read=string.rot13/resource=" . "raw.txt";
var_dump(copy($src, "out.txt"));
echo file_get_contents("out.txt"), "\n";
unlink("raw.txt");
unlink("out.txt");
"#,
    );
    assert_eq!(out, "bool(true)\nhello\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies the same URL written as a LITERAL still works, so the two spellings agree.
#[test]
fn test_a_literal_filter_url_is_copied_through_its_chain() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("raw.txt", "uryyb");
var_dump(copy("php://filter/read=string.rot13/resource=raw.txt", "out.txt"));
echo file_get_contents("out.txt"), "\n";
unlink("raw.txt");
unlink("out.txt");
"#,
    );
    assert_eq!(out, "bool(true)\nhello\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies every ordinary run-time copy still behaves, now that it passes a filter probe first.
///
/// The self-copy is the one php decides by `(st_dev, st_ino)` rather than by comparing paths, and
/// an EMPTY source is a SUCCESS — both live in `__rt_copy`, which the fall-through still reaches.
#[test]
fn test_an_ordinary_run_time_copy_is_unchanged() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("src.txt", "payload");
$src = "src.txt";
$dst = "dst.txt";
var_dump(copy($src, $dst));
echo file_get_contents($dst), "\n";
var_dump(copy($src, $src));
echo file_get_contents($src), "\n";
$missing = "gone-forever.txt";
var_dump(@copy($missing, $dst));
file_put_contents("empty.txt", "");
$e = "empty.txt";
var_dump(copy($e, $dst));
var_dump(file_get_contents($dst));
unlink("src.txt");
unlink("dst.txt");
unlink("empty.txt");
"#,
    );
    assert_eq!(
        out,
        "bool(true)\npayload\nbool(false)\npayload\nbool(false)\nbool(true)\nstring(0) \"\"\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}
