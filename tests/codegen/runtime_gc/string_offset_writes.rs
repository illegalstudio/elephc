//! Purpose:
//! Heap-debug coverage for PHP string offset writes (`$s[$i] = $v`, issue #851).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A string offset write builds a new string and stores it over the old one, so every write
//!   retires one string and every copy that shared the old one must keep it alive. The loops
//!   run hundreds of writes, so a per-write leak cannot hide in heap slack, and they read the
//!   shared copies back, so an over-release shows up as corrupted output.
//! - `up()` returns its reassigned `string` parameter: its result is fresh storage, not the
//!   caller's argument, which is what the return-alias summary has to know to release it.
//! - Expected stdout values are real PHP 8.5 output for the same fixtures.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Local writes, copies, padding, illegal offsets, caught errors, the expression form, and
/// function parameters all balance.
#[test]
fn test_string_offset_writes_balance_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function up(string $w): string { $w[0] = strtoupper($w[0]); return $w; }
function cp(string $w): string { $x = $w; $x[1] = 'C'; return $x; }
function third(string $w): string { return ($w[2] = 'E'); }
$keep = '';
for ($i = 0; $i < 300; $i++) {
    $s = str_repeat('ab', 8) . $i;
    $t = $s;
    $s[3] = 'X';
    $s[20] = (string)($i % 10);
    $s[-1] = 'Q';
    $s[-99] = 'q';
    $r = ($s[0] = 'multi');
    try { $s[1] = ''; } catch (Error $e) { }
    $keep = $s . '|' . $t . '|' . $r . '|' . up($t) . '|' . cp($t) . '|' . third($t) . '|' . up($s . '!');
}
echo $keep, "\n";
"#,
    );
    assert_clean(
        out,
        "mbaXabababababab299 Q|abababababababab299|m|Abababababababab299|aCababababababab299|E|MbaXabababababab299 Q!\n",
    );
}

/// Writes into boxed `mixed` storage replace the cell's string payload without leaking the old
/// one, the value box, or the cast value, including the TypeError and empty-value paths.
#[test]
fn test_string_offset_writes_on_mixed_storage_balance_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function m(mixed $v): mixed { $v[1] = 'M'; $v[9] = 7; return $v; }
function e(mixed $v): mixed { try { $v[0] = ''; } catch (Error $x) { return 'err'; } return $v; }
function k(mixed $v): mixed { try { $v['x'] = 'a'; } catch (TypeError $x) { return 'type'; } return $v; }
$out = '';
for ($i = 0; $i < 300; $i++) {
    $s = 'str' . $i;
    $out = m($s) . '|' . e($s) . '|' . k($s) . '|' . $s;
}
echo $out, "\n";
"#,
    );
    assert_clean(out, "sMr299   7|err|type|str299\n");
}

/// A write whose result outgrows the 64 KiB concat scratch buffer releases its heap-backed
/// temporary.
#[test]
fn test_string_offset_write_large_string_balance_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = 0;
for ($i = 0; $i < 20; $i++) {
    $big = str_repeat('z', 70000);
    $big[70000 + $i] = 'E';
    $n += strlen($big);
}
echo $n, "\n";
"#,
    );
    assert_clean(out, "1400210\n");
}
