//! Purpose:
//! Codegen regression tests for list unpack and keyed destructuring over
//! associative arrays (and a superglobal). Verifies that positional unpack
//! reads integer keys 0..n-1 through the hash path (returning null on miss)
//! and that keyed destructuring (desugared at parse time) reads named keys.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions
//!   compare stdout.

use super::*;

/// Positional unpack over an assoc with int key 0 present reads that slot.
#[test]
fn test_list_unpack_assoc_int_key_zero_hit() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 'x'];
$m[0] = 'zero';
[$v] = $m;
echo $v;
"#,
    );
    assert_eq!(out, "zero");
}

/// Positional unpack over an assoc without int key 0 yields null (PHP
/// semantics: missing key produces null + undefined-key notice).
#[test]
fn test_list_unpack_assoc_miss_is_null() {
    let out = compile_and_run(
        r#"<?php
$m = ['a' => 'x'];
[$v] = $m;
echo $v === null ? 'null' : 'set';
"#,
    );
    assert_eq!(out, "null");
}

/// Keyed destructuring (parse-time desugared) over an assoc literal reads
/// the named slots.
#[test]
fn test_keyed_destructuring_assoc() {
    let out = compile_and_run(
        r#"<?php
['name' => $n, 'city' => $c] = ['name' => 'Bob', 'city' => 'Paris'];
echo $n . '|' . $c;
"#,
    );
    assert_eq!(out, "Bob|Paris");
}

/// Keyed destructuring over a superglobal reads the named slot.
#[test]
fn test_keyed_destructuring_superglobal() {
    let out = compile_and_run(
        r#"<?php
$_GET = ['q' => 'search'];
['q' => $q] = $_GET;
echo $q;
"#,
    );
    assert_eq!(out, "search");
}

/// Two-positional unpack over an assoc with int keys 0 and 1 reads both
/// slots in order.
#[test]
fn test_list_unpack_two_positional_assoc() {
    let out = compile_and_run(
        r#"<?php
$m = ['x' => '1'];
$m[0] = 'a';
$m[1] = 'b';
[$p, $q] = $m;
echo $p . $q;
"#,
    );
    assert_eq!(out, "ab");
}
