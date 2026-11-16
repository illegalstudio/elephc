//! Purpose:
//! End-to-end coverage for locals a read may find UNASSIGNED: a variable stored on one branch
//! only, read before its first store in a loop, or created by an array element write in a loop
//! that may not run. php answers a plain read with `Warning: Undefined variable $x` and NULL,
//! `isset()`/`empty()`/`??` with "not set" and no warning, and `compact()` by leaving the name
//! out with `compact(): Undefined variable $x`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs are php 8.5's (`php -n`) with diagnostics interleaved, normalized by
//!   [`normalized`]: php prints a blank line before each diagnostic and names the script's path,
//!   which differs per test directory.
//! - Every fixture takes its branch from a runtime value (`$argc`, a parameter) so constant
//!   folding cannot decide the definedness at compile time.

use crate::support::*;

/// Compiles and runs `source`, returning stdout and diagnostics merged in write order, with each
/// diagnostic's ` in FILE` dropped so ` on line N` stays comparable.
fn normalized(source: &str) -> String {
    let out = compile_and_run_capture_merged(source);
    assert!(out.success, "program failed: {}", out.stdout);
    out.stdout
        .lines()
        .map(|line| match (line.find(" in /"), line.rfind(" on line ")) {
            (Some(start), Some(end)) if start < end => format!("{}{}", &line[..start], &line[end..]),
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A local stored on one branch only: compact() leaves it out with php's warning, isset() and
/// `??` see it unset, and a plain read warns and yields NULL (the review's reproducer, which
/// printed `int(6097806336)` for the read).
#[test]
fn test_branch_assigned_local_is_undefined_on_the_other_path() {
    let out = normalized(
        r#"<?php
function f($argc) {
    if ($argc > 5) { $d = 1; }
    var_dump(compact('d'));
    var_dump(isset($d));
    var_dump($d ?? 'def');
    var_dump($d);
}
f($argc);
f(9);
"#,
    );
    assert_eq!(
        out,
        "Warning: compact(): Undefined variable $d on line 4
array(0) {
}
bool(false)
string(3) \"def\"
Warning: Undefined variable $d on line 7
NULL
array(1) {
  [\"d\"]=>
  int(1)
}
bool(true)
int(1)
int(1)"
    );
}

/// A string local read after a loop that keeps the tail from being duplicated into the arms.
#[test]
fn test_branch_assigned_string_read_after_a_loop() {
    let out = normalized(
        r#"<?php
function g($argc) {
    if ($argc > 5) { $s = "x"; }
    for ($i = 0; $i < 1; $i++) { echo "loop\n"; }
    var_dump(isset($s), $s ?? 'def', empty($s));
    echo "[" . $s . "]\n";
}
g($argc);
g(7);
"#,
    );
    assert_eq!(
        out,
        "loop
bool(false)
string(3) \"def\"
bool(true)
Warning: Undefined variable $s on line 6
[]
loop
bool(true)
string(1) \"x\"
bool(false)
[x]"
    );
}

/// A local read in a loop before the iteration that first stores it.
#[test]
fn test_loop_carried_local_read_before_its_first_store() {
    let out = normalized(
        r#"<?php
function h($n) {
    for ($i = 0; $i < $n; $i++) {
        var_dump(isset($d), $d);
        $d = $i + 10;
    }
}
h(2);
"#,
    );
    assert_eq!(
        out,
        "Warning: Undefined variable $d on line 4
bool(false)
NULL
bool(true)
int(10)"
    );
}

/// `$out[] = …` in a loop that does not run leaves `$out` undefined: the array exists only
/// once the first element write has run (it was created at function entry, `array(0)`).
#[test]
fn test_array_local_created_by_element_writes_exists_only_after_one() {
    let out = normalized(
        r#"<?php
function collect($xs) { foreach ($xs as $x) { $out[] = $x; } var_dump($out ?? null); }
collect([]);
collect([1, 2]);
function keyed($xs) {
    foreach ($xs as $k => $x) { $m[$k] = $x; }
    var_dump(isset($m), empty($m), $m ?? 'none', compact('m'));
}
keyed([]);
keyed(['a' => 1]);
"#,
    );
    assert_eq!(
        out,
        "NULL
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
Warning: compact(): Undefined variable $m on line 7
bool(false)
bool(true)
string(4) \"none\"
array(0) {
}
bool(true)
bool(false)
array(1) {
  [\"a\"]=>
  int(1)
}
array(1) {
  [\"m\"]=>
  array(1) {
    [\"a\"]=>
    int(1)
  }
}"
    );
}

/// Definedness through a `try` whose body may throw before the store, a `switch` arm, a
/// compound assignment and an increment of an undefined local (both read it first), closure
/// captures (`use` warns, an arrow function does not), loops that may not run, and a `list()`
/// store on one branch.
#[test]
fn test_definedness_across_try_switch_increments_captures_and_loops() {
    let out = normalized(
        r#"<?php
function thrower($t) { if ($t) { throw new Exception("x"); } return 7; }
function a($t) {
    try { $x = thrower($t); } catch (Exception $e) { echo "caught\n"; }
    var_dump($x ?? 'none');
    var_dump(isset($e));
}
a($argc < 0); a($argc > 0);
function b($c) {
    switch ($c) { case 1: $s = 'one'; break; case 2: $s = 'two'; break; }
    echo $s ?? 'dflt', "\n";
}
b($argc); b($argc + 2);
function c($c) {
    if ($c) { $str = 'a'; }
    $str .= 'b';
    echo $str, "\n";
    if ($c) { $n = 1; }
    $n++;
    var_dump($n);
}
c($argc > 0); c($argc < 0);
function d($c) {
    if ($c) { $v = 5; }
    $f = fn() => $v ?? 'nov';
    echo $f(), "\n";
    $g = function () use ($c) { return $c; };
    echo $g() ? "t" : "f", "\n";
}
d($argc > 0); d($argc < 0);
function e($xs) {
    foreach ($xs as &$r) { $r *= 2; }
    unset($r);
    var_dump($xs);
    foreach ($xs as $k => $v) { $last = $v; }
    var_dump($last ?? 'empty', $k ?? 'nok');
}
e([1, 2]); e([]);
function g($c) {
    static $cnt = 0;
    $cnt++;
    if ($c) { [$p, $q] = [1, 2]; }
    var_dump($p ?? 'np', $cnt);
}
g($argc > 0); g($argc < 0);
"#,
    );
    assert_eq!(
        out,
        "int(7)
bool(false)
caught
string(4) \"none\"
bool(true)
one
dflt
ab
int(2)
Warning: Undefined variable $str on line 16
b
Warning: Undefined variable $n on line 19
int(1)
5
t
nov
f
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(4)
}
int(4)
int(1)
array(0) {
}
string(5) \"empty\"
string(3) \"nok\"
int(1)
int(1)
string(2) \"np\"
int(2)"
    );
}

/// A plain read of such a local before any write warns like php. Its value stays the empty
/// array: the checker types the local `array` for the whole scope, so the returned NULL php has
/// would not fit the function's `array` return.
#[test]
fn test_array_local_created_by_element_writes_warns_on_a_plain_read() {
    let out = normalized(
        r#"<?php
function rows($xs) { foreach ($xs as $x) { $rows[] = $x; } return $rows; }
$none = rows([]);
echo "after\n", count(rows([1, 2])), "\n";
"#,
    );
    assert_eq!(out, "Warning: Undefined variable $rows on line 2\nafter\n2");
}

/// Every way php defines a variable inside a branch is seen as a definition after it, and a
/// defined `null` stays in compact()'s result. On the untaken branch nothing is defined.
#[test]
fn test_definitions_on_one_branch_are_seen_after_the_merge() {
    let out = normalized(
        r#"<?php
function byref(&$o) { $o = 5; }
function g($c) {
    if ($c) {
        preg_match('/(b)/', 'abc', $m);
        byref($r);
        [$a] = [1];
        foreach ([5] as $v) {}
        $s = 'x';
        $s .= 'y';
        $arr['k'] = 1;
        $n = null;
    }
    var_dump($m ?? 'u', $r ?? 'u', $a ?? 'u', $v ?? 'u', $s ?? 'u', $arr ?? 'u');
    var_dump(isset($n), compact('n', 's'));
}
g($argc > 0);
g($argc < 0);
"#,
    );
    assert_eq!(
        out,
        "array(2) {
  [0]=>
  string(1) \"b\"
  [1]=>
  string(1) \"b\"
}
int(5)
int(1)
int(5)
string(2) \"xy\"
array(1) {
  [\"k\"]=>
  int(1)
}
bool(false)
array(2) {
  [\"n\"]=>
  NULL
  [\"s\"]=>
  string(2) \"xy\"
}
string(1) \"u\"
string(1) \"u\"
string(1) \"u\"
string(1) \"u\"
string(1) \"u\"
string(1) \"u\"
Warning: compact(): Undefined variable $n on line 15
Warning: compact(): Undefined variable $s on line 15
bool(false)
array(0) {
}"
    );
}
