//! Purpose:
//! Integration tests for PHP string offset writes (`$s[$i] = $v`, issue #851).
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Expected stdout and warning texts are real PHP 8.5 output for the same fixtures.
//! - Warnings go to stderr in elephc, so fixtures that warn capture both streams.
//! - Fixtures cover plain string locals and the boxed `mixed` storage path (a `mixed`
//!   parameter, a `global`, and a string local whose `++` gives it boxed storage).

use crate::support::*;

/// Writes one byte, pads past the end, counts negative offsets from the end, and converts
/// non-string values the way PHP does.
#[test]
fn test_string_offset_write_basic_semantics() {
    let out = compile_and_run(
        r#"<?php
$s = 'abc'; $s[1] = 'Z'; echo $s, "\n";
$s = 'abc'; $s[5] = 'x'; echo "[$s]", strlen($s), "\n";
$s = 'abc'; $s[-1] = 'q'; echo $s, "\n";
$s = 'abc'; $s[-3] = 'q'; echo $s, "\n";
$s = ''; $s[3] = 'z'; echo "[$s]\n";
$s = 'abc'; $s["1"] = 'N'; echo $s, "\n";
$s = 'abc'; $s[" 1"] = 'W'; echo $s, "\n";
$s = 'abc'; $s[0] = 5; echo $s, "\n";
$s = 'abc'; $s[0] = true; echo $s, "\n";
$s = 'abc'; $s[1] = "\0"; echo bin2hex($s), "\n";
$s = 'abc'; $s[0] = 'x'; $s[0] = 'y'; echo $s, "\n";
$m = 'aaaa';
for ($k = 0; $k < 4; $k++) { $m[$k] = chr(65 + $k); }
echo $m, "\n";
"#,
    );
    assert_eq!(
        out,
        "aZc\n[abc  x]6\nabq\nqbc\n[   z]\naNc\naWc\n5bc\n1bc\n610063\nybc\nABCD\n"
    );
}

/// Runtime-only offsets and values take the same path as literal ones.
#[test]
fn test_string_offset_write_runtime_operands() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat('-', $argc + 3);
$i = $argc;
$s[$i] = 'M';
$s[$i + 2] = (string)($argc * 7);
$s[-$argc - 3] = 'E';
$s[$argc + 5] = 'P';
echo $s, "\n";
"#,
    );
    assert_eq!(out, "EM-7  P\n");
}

/// PHP's three string offset write warnings: a multi-byte value, an offset before the start
/// (which writes nothing), and a float offset.
#[test]
fn test_string_offset_write_warnings() {
    let out = compile_and_run_capture(
        r#"<?php
$s = 'abc'; $s[1] = 'XYZ'; echo $s, "\n";
$s = 'abc'; $s[-4] = 'q'; echo $s, "\n";
$s = 'abc'; $s[1.5] = 'F'; echo $s, "\n";
"#,
    );
    assert_eq!(out.stdout, "aXc\nabc\naFc\n");
    assert!(
        out.stderr
            .contains("Warning: Only the first byte will be assigned to the string offset"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("Warning: Illegal string offset -4"), "{}", out.stderr);
    assert!(out.stderr.contains("Warning: String offset cast occurred"), "{}", out.stderr);
}

/// An empty value throws a catchable `Error`; `null` converts to the empty string and does too.
/// An illegal offset is checked first, so it only warns even for an empty value.
#[test]
fn test_string_offset_write_empty_value_throws_error() {
    let out = compile_and_run_capture(
        r#"<?php
$s = 'abc';
try { $s[0] = ''; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $s[1] = null; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { $s[-9] = ''; } catch (Error $e) { echo "unexpected\n"; }
echo $s, "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "Error: Cannot assign an empty string to a string offset\n\
         Error: Cannot assign an empty string to a string offset\nabc\n"
    );
    assert!(out.stderr.contains("Warning: Illegal string offset -9"), "{}", out.stderr);
}

/// An uncaught empty-value write ends the program with PHP's uncaught `Error` report.
#[test]
fn test_string_offset_write_uncaught_empty_value() {
    let out = compile_and_run_capture("<?php $s = 'abc'; $s[0] = ''; echo 'not reached';");
    assert!(!out.stdout.contains("not reached"), "{}", out.stdout);
    let report = format!("{}{}", out.stdout, out.stderr);
    assert!(
        report.contains("Uncaught Error: Cannot assign an empty string to a string offset"),
        "{}",
        report
    );
}

/// Another variable, a caller's argument, and a by-value copy keep the old bytes.
#[test]
fn test_string_offset_write_is_copy_on_write() {
    let out = compile_and_run(
        r#"<?php
function up(string $w): string { $w[0] = strtoupper($w[0]); return $w; }
$orig = 'hello';
$copy = $orig;
$copy[4] = '!';
echo up($orig), ' ', $orig, ' ', $copy, "\n";
$list = ['one', 'two'];
foreach ($list as $v) { $v[0] = 'X'; echo $v, ' '; }
echo implode(',', $list), "\n";
"#,
    );
    assert_eq!(out, "Hello hello hell!\nXne Xwo one,two\n");
}

/// The assignment expression evaluates to the byte actually written.
#[test]
fn test_string_offset_write_expression_value() {
    let out = compile_and_run_capture(
        r#"<?php
$s = 'abc';
$r = ($s[1] = 'hello');
echo $r, '|', $s, "\n";
function third(string $w): string { return ($w[2] = 'E'); }
echo third('xyz'), "\n";
"#,
    );
    assert_eq!(out.stdout, "h|ahc\nE\n");
}

/// A reference-bound string, a static local, and a closure parameter all take the write.
#[test]
fn test_string_offset_write_through_other_local_kinds() {
    let out = compile_and_run(
        r#"<?php
function counter(): string { static $c = 'aaa'; $c[1] = 'b'; $c = $c . 'x'; return $c; }
echo counter(), ' ', counter(), "\n";
function byref(string &$s): void { $s[0] = 'R'; }
$b = 'bref'; $alias = $b; byref($b); echo $b, ' ', $alias, "\n";
$ref = 'refd'; $r2 = &$ref; $r2[0] = 'Z'; echo $ref, "\n";
$cl = function (string $x): string { $x[0] = 'C'; return $x; };
echo $cl('closure'), "\n";
"#,
    );
    assert_eq!(out, "abax abaxx\nRref bref\nZefd\nClosure\n");
}

/// A string held in boxed `mixed` storage takes the write too; a string key throws PHP's
/// `TypeError`.
#[test]
fn test_string_offset_write_on_mixed_storage() {
    let out = compile_and_run_capture(
        r#"<?php
function m(mixed $v): mixed { $v[1] = 'M'; return $v; }
function k(mixed $v): mixed { try { $v['x'] = 'a'; } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; } return $v; }
$g = 'glob';
function touch_global(): void { global $g; $g[0] = 'G'; }
touch_global();
$inc = 'az'; $inc[0] = 'b'; echo $inc; $inc++; echo ' ', $inc, "\n";
echo m('plain'), ' ', k('keyed'), ' ', $g, "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        "bz ca\npMain TypeError: Cannot access offset of type string on string\nkeyed Glob\n"
    );
}

/// A write past the 64 KiB concat scratch buffer still pads and writes correctly.
#[test]
fn test_string_offset_write_large_string() {
    let out = compile_and_run(
        r#"<?php
$big = str_repeat('z', 70000);
$big[70005] = 'E';
$big[0] = 'A';
echo strlen($big), substr($big, 0, 2), '[', substr($big, -7), "]\n";
"#,
    );
    assert_eq!(out, "70006Az[z     E]\n");
}
