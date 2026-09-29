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

/// A warning handler that replaces the global being written leaves it with the handler's
/// value, like php, which abandons the write; the retained cell is released without a leak.
#[test]
fn test_string_offset_write_on_global_replaced_by_warning_handler() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function h(int $no, string $msg): bool { global $g; $g = str_repeat('Z', 12); return true; }
function run(): string {
    global $g;
    $seen = '';
    for ($i = 0; $i < 300; $i++) {
        $g = str_repeat('abcdefgh', 4) . $i;
        $g[1] = 'xy';
        $seen = $g;
    }
    return $seen;
}
$g = '';
set_error_handler('h');
$last = run();
restore_error_handler();
echo $last, ' ', $g, "\n";
$g = null;
"#,
    );
    assert_clean(out, "ZZZZZZZZZZZZ ZZZZZZZZZZZZ\n");
}

/// A warning handler that frees the string being written (here a static local, replaced by a
/// recursive call) cannot make the write read freed bytes. php abandons the write in that
/// case and elephc keeps the written copy, so the fixture only checks the result is made of
/// bytes the program wrote; heap debug poisons freed blocks, which a stale read would copy.
#[test]
fn test_string_offset_write_survives_warning_handler_freeing_subject() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function subject(bool $replace): string {
    static $s = '';
    if ($replace) {
        $s = str_repeat('Z', 40);
        return '';
    }
    $s = str_repeat('abcdefgh', 5);
    $s[1] = 'xy';
    return $s;
}
function replace_subject(int $no, string $msg): bool { subject(true); $junk = str_repeat('Q', 40); return true; }
set_error_handler('replace_subject');
$last = '';
for ($i = 0; $i < 50; $i++) {
    $last = subject(false);
}
restore_error_handler();
echo strlen($last), ' ', trim($last, 'abcdefghxZ') === '' ? 'intact' : 'corrupt', "\n";
"#,
    );
    assert_eq!(out.stdout, "40 intact\n", "stderr: {}", out.stderr);
}

/// The expression form balances on a string local and on boxed storage, including the
/// illegal-offset (`null`) arm and the array arm of a `mixed` value. `pick()` returns the
/// expression over a value parameter: the result is a fresh byte, not that parameter, which
/// the return-alias summary has to know for the caller to release it.
#[test]
fn test_string_offset_write_expression_balances_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function first(mixed $s): mixed { return ($s[0] = 'hello'); }
function neg(mixed $s): mixed { return ($s[-99] = 'x'); }
function keyed(mixed $a): mixed { return ($a['k'] = [1, 2]); }
function pick(string $s, string $v) { return ($s[1] = $v); }
$out = '';
for ($i = 0; $i < 300; $i++) {
    $s = str_repeat('abc', 2) . $i;
    $r = ($s[$i % 4] = 'multi');
    $n = ($s[-99] = 'q');
    $v = 'v' . $i;
    $out = $r . '|' . var_export($n, true) . '|' . $s . '|' . first($s) . '|'
        . var_export(neg($s), true) . '|' . json_encode(keyed(null)) . '|' . json_encode(first([$i]))
        . '|' . pick($s, $v) . $v;
}
echo $out, "\n";
"#,
    );
    assert_clean(out, "m|NULL|abcmbc299|h|NULL|[1,2]|\"hello\"|vv299\n");
}

/// A write whose result outgrows the 64 KiB concat scratch buffer releases its heap-backed
/// temporary. The subject is built by `.=` rather than one `str_repeat()` call: a
/// `str_repeat()` result past the scratch buffer is not released once stored, independently
/// of string offset writes, and would hide this fixture's own balance.
#[test]
fn test_string_offset_write_large_string_balance_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$chunk = str_repeat('z', 1000);
$n = 0;
for ($i = 0; $i < 20; $i++) {
    $big = '';
    for ($k = 0; $k < 70; $k++) {
        $big .= $chunk;
    }
    $big[70000 + $i] = 'E';
    $n += strlen($big);
}
echo $n, "\n";
"#,
    );
    assert_clean(out, "1400210\n");
}

/// Returns the `live_blocks` count from a `--heap-debug` leak summary.
fn live_blocks(stderr: &str) -> usize {
    stderr
        .split("leak summary: live_blocks=")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

/// A warning handler that THROWS out of a string offset write, and an empty value written at
/// `PHP_INT_MAX`, release everything the write held: the retained cell, the cast value and the
/// helper's private subject copy. Before the fix each iteration stranded two to four blocks, so
/// the live count at exit grew with the iteration count. The handler's own state stays live at
/// exit either way, so the test compares 5 and 50 iterations rather than asking for a clean
/// summary. Review follow-up for #851.
#[test]
fn test_string_offset_write_releases_holds_when_a_warning_handler_throws() {
    let program = |count: usize| {
        format!(
            r#"<?php
function raise(int $no, string $msg): bool {{
    throw new RuntimeException($msg);
}}
set_error_handler("raise");
function once(int $i, int $argc): int {{
    $s = $argc > 99 ? [] : "abc" . $i;
    $c = 0;
    try {{ $s[PHP_INT_MAX] = ""; }} catch (Error $e) {{ $c++; }}
    try {{ $s[-10] = "x"; }} catch (RuntimeException $e) {{ $c++; }}
    try {{ $s[-10] = str_repeat("y", 3) . $i; }} catch (RuntimeException $e) {{ $c++; }}
    try {{ $s[1] = "long" . $i; }} catch (RuntimeException $e) {{ $c++; }}
    return $c * 100 + strlen($s);
}}
$t = 0;
for ($i = 0; $i < {count}; $i++) {{ $t += once($i, $argc); }}
echo $t, "\n";
"#
        )
    };
    let few = compile_and_run_with_heap_debug(&program(5));
    let many = compile_and_run_with_heap_debug(&program(50));
    assert_eq!(few.stdout, "2020\n", "stderr: {}", few.stderr);
    assert_eq!(many.stdout, "20240\n", "stderr: {}", many.stderr);
    assert_eq!(
        live_blocks(&few.stderr),
        live_blocks(&many.stderr),
        "live blocks grew with the iteration count:\n{}\n{}",
        few.stderr,
        many.stderr
    );
}
