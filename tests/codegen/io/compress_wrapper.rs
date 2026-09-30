//! Purpose:
//! Integration tests for the `compress.zlib://` / `compress.bzip2://` wrappers reached through a
//! URL or a mode that is only known at RUN time.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Both halves of an `fopen()` used to have to be compile-time literals for these wrappers to
//!   work properly. A computed URL reached the plain byte readers as a FILENAME, and a computed
//!   MODE was classified as "r" whatever it said — so `$m = "w"; fopen("compress.zlib://o.gz", $m)`
//!   opened for reading and warned about a file it was asked to create. Neither warned about the
//!   real problem; both are ordinary php.
//! - php's zlib wrapper classifies on the FIRST character of the mode and refuses any `+`. The
//!   refusals are pinned as well as the successes, because a classifier that accepts everything
//!   passes every happy-path test.
//! - Every expectation was MEASURED on `php -n` 8.5.6.
//! - The archives are written by the tests themselves, so no binary fixture is checked in.

use crate::support::*;

/// Verifies a computed mode of `"w"` actually opens for WRITING.
///
/// This is the silent case: the open answered a readable stream on a file that did not exist, so
/// the failure surfaced as a warning naming a missing file rather than as a wrong direction.
#[test]
fn test_a_computed_write_mode_opens_the_compress_wrapper_for_writing() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$m = "w";
$h = fopen("compress.zlib://out.gz", $m);
var_dump(fwrite($h, "hello\n"));
fclose($h);
var_dump(file_get_contents("compress.zlib://out.gz"));
"#,
    );
    assert_eq!(out, "int(6)\nstring(6) \"hello\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies the mode's compression-level suffix does not change the classification.
///
/// `wb9` is `w` as far as the direction goes; php reads only the first character.
#[test]
fn test_a_computed_mode_with_a_level_suffix_still_writes() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$write = true;
$m = $write ? "wb9" : "rb";
$h = fopen("compress.zlib://lvl.gz", $m);
var_dump(fwrite($h, "abc"));
fclose($h);
var_dump(file_get_contents("compress.zlib://lvl.gz"));
"#,
    );
    assert_eq!(out, "int(3)\nstring(3) \"abc\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a computed `"a"` appends rather than truncating.
#[test]
fn test_a_computed_append_mode_appends() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("compress.zlib://app.gz", "w");
fwrite($h, "one\n");
fclose($h);
$m = "a";
$h = fopen("compress.zlib://app.gz", $m);
var_dump(fwrite($h, "two\n"));
fclose($h);
"#,
    );
    assert_eq!(out, "int(4)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies the modes php REFUSES are refused when they are computed too.
///
/// `r+`, `w+`, `x`, `c` and the empty string all answer `false` in `php -n` 8.5.6 — `c` even
/// though the plain-file wrapper accepts it — while `rw` SUCCEEDS, because only the first
/// character is read. A classifier that ignored the rule would pass every other test in this file.
#[test]
fn test_computed_modes_php_refuses_are_refused() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("compress.zlib://src.gz", "w");
fwrite($h, "payload");
fclose($h);
foreach (["r+", "w+", "x", "c", ""] as $m) {
    var_dump(@fopen("compress.zlib://src.gz", $m));
}
$m = "rw";
var_dump(is_resource(@fopen("compress.zlib://src.gz", $m)));
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `file_get_contents()` reads a compress URL assembled at run time.
///
/// The literal spelling already worked, so the two spellings of one read disagreed: this answered
/// `Failed to open stream: No such file or directory`, naming a path no file has.
#[test]
fn test_file_get_contents_reads_a_computed_compress_url() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("compress.zlib://dyn.gz", "w");
fwrite($h, "one\ntwo\n");
fclose($h);
$p = "dyn.gz";
var_dump(file_get_contents("compress.zlib://" . $p));
"#,
    );
    assert_eq!(out, "string(8) \"one\ntwo\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `file()` and `readfile()` read a computed compress URL as well.
///
/// They have their own byte-consuming tails — one splits into lines, the other writes and counts —
/// which is why each needed the route rather than sharing one landing point.
#[test]
fn test_file_and_readfile_read_a_computed_compress_url() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("compress.zlib://dyn2.gz", "w");
fwrite($h, "alpha\nbeta\n");
fclose($h);
$p = "dyn2.gz";
var_dump(count(file("compress.zlib://" . $p)));
var_dump(readfile("compress.zlib://" . $p));
"#,
    );
    assert_eq!(out, "int(2)\nalpha\nbeta\nint(11)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a computed URL that names NO wrapper still reads as an ordinary file.
///
/// The run-time probe runs in front of the plain reader, so this pins that it restores what it
/// found: a plain path must reach the reader untouched.
#[test]
fn test_a_computed_plain_path_is_unaffected_by_the_wrapper_probe() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("plain.txt", "just bytes\n");
$p = "plain.txt";
var_dump(file_get_contents($p));
var_dump(count(file($p)));
"#,
    );
    assert_eq!(out, "string(11) \"just bytes\n\"\nint(1)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a `?int $length = null` PARAMETER can be forwarded to `fgets` and `fwrite`.
///
/// php's own signatures spell these `?int $length = null`, so forwarding one is what any wrapper
/// function does. It did not compile: a `?int` local is a (value, tag) pair, and the resolvers
/// refused it with `unsupported EIR backend feature: fgets length for PHP type TaggedScalar`.
#[test]
fn test_a_nullable_int_parameter_can_be_forwarded_to_the_stream_builtins() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function put(mixed $h, string $d, ?int $n = null): int|false {
    if ($n === null) {
        return fwrite($h, $d);
    }
    return fwrite($h, $d, $n);
}
function line(mixed $h, ?int $n = null): string|false {
    if ($n === null) {
        return fgets($h);
    }
    return fgets($h, $n);
}
$h = fopen("php://memory", "w+");
var_dump(put($h, "abcdef\n"));
var_dump(put($h, "ghijkl\n", 3));
rewind($h);
var_dump(line($h));
var_dump(line($h, 3));
fclose($h);
"#,
    );
    assert_eq!(
        out,
        "int(7)\nint(3)\nstring(7) \"abcdef\n\"\nstring(2) \"gh\"\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
