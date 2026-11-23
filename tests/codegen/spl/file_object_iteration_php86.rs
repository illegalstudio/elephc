//! Purpose:
//! Runs PHP 8.6's SplFileObject iteration/seek semantics fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests file_object_iteration_php86`.
//!
//! Key details:
//! - PHP 8.6 stops `next()` from growing `key()` past EOF, advances the stream
//!   when no `current()` cached a line (so a following `fgets()` reads the new
//!   line), and clamps `seek()` past EOF to the same key as SplTempFileObject.
//! - The pre-8.6 profile keeps the old unbounded/past-EOF values.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// The fixture body shared by both profile checks.
const SOURCE: &str = r#"<?php
$path = 'data.txt';
file_put_contents($path, "a\nb\nc\n");
$f = new SplFileObject($path);
echo $f->key(), ':', trim($f->current()), '|';
$f->next();
echo $f->key(), ':', trim($f->current()), '|';
$f->next();
$f->next();
echo $f->key(), ':', var_export($f->current(), true), '|';
$f->next();
echo $f->key(), '|';
$f->seek(100);
echo $f->key(), '|';
$f->seek(1);
echo $f->key(), ':', trim($f->current()), ':', trim($f->fgets()), ':', $f->key(), '|';
$t = new SplTempFileObject();
$t->fwrite("a\nb\nc\n");
$t->seek(100);
echo $t->key();
"#;

/// Verifies the 8.6 EOF/stream/seek behavior and the pre-8.6 values.
#[test]
fn test_spl_file_object_iteration_php86() {
    let out86 = compile_and_run_with_php_version(SOURCE, PhpVersion::Php86);
    assert_eq!(out86, "0:a|1:b|3:false|3|3|1:b:b:2|3");

    let out85 = compile_and_run_with_php_version(SOURCE, PhpVersion::Php85);
    assert!(
        out85.contains("|4|100|"),
        "PHP 8.5 keeps the unbounded next() key and the past-EOF seek value: {out85}"
    );
}
