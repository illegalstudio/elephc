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
    let lines: Vec<&str> = out.stdout.lines().collect();
    lines
        .iter()
        .enumerate()
        // php prints a blank line before each diagnostic (`\nWarning: ...`); the fixtures spell
        // the diagnostics without it.
        .filter(|(at, line)| {
            !(line.is_empty()
                && lines.get(at + 1).is_some_and(|next| {
                    next.starts_with("Warning: ") || next.starts_with("Notice: ")
                        || next.starts_with("Deprecated: ")
                }))
        })
        .map(|(_, line)| match (line.find(" in /"), line.rfind(" on line ")) {
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

/// The same read-before-store shape through a `while`, a `??` probe and a bare `isset`: every
/// later iteration must see what the earlier one stored (output of `php -n`).
#[test]
fn test_loop_carried_local_probes_see_the_previous_iterations_store() {
    let out = normalized(
        r#"<?php
function w($n) {
    $i = 0;
    while ($i < $n) {
        var_dump(isset($d), $d);
        $d = $i + 10;
        $i++;
    }
}
w(2);
function g($n) {
    for ($i = 0; $i < $n; $i++) {
        var_dump($d ?? "u");
        $d = "s$i";
    }
}
g(2);
function k($n) {
    for ($i = 0; $i < $n; $i++) {
        var_dump(isset($d));
        $d = "s$i";
    }
}
k(2);
"#,
    );
    assert_eq!(
        out,
        "Warning: Undefined variable $d on line 5
bool(false)
NULL
bool(true)
int(10)
string(1) \"u\"
string(2) \"s0\"
bool(false)
bool(true)"
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

/// An object stored inside a `try` is possibly undefined after it, but a read through it keeps
/// its typed dispatch: `empty($box['q'])` still asks `offsetExists()` before `offsetGet()`.
#[test]
fn test_object_from_a_try_keeps_its_array_access_dispatch() {
    let out = normalized(
        r#"<?php
class Box implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { echo "E"; return $offset === "p"; }
    public function offsetGet(mixed $offset): mixed { echo "G"; throw new Exception("x"); }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
try {
    $box = new Box();
    $value = $box["p"];
} catch (Exception $e) {
    echo "|caught\n";
}
echo empty($box["q"]) ? "|empty\n" : "|set\n";
var_dump(isset($value));
"#,
    );
    assert_eq!(out, "G|caught\nE|empty\nbool(false)");
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

/// Runs `source` twice and compares it with php 8.5 (`php -n`): once with diagnostics merged in
/// write order (`merged`, normalized like [`normalized`]) and once under heap debug, where the
/// program's stdout alone must equal php's (`plain`, diagnostics off) and the heap must end clean,
/// so the boxed reads these fixtures produce neither leak nor double-free.
pub(super) fn checked(source: &str, merged: &str, plain: &str) {
    assert_eq!(normalized(source), merged);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "heap-debug run failed: {}", out.stderr);
    assert_eq!(out.stdout, plain);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A local assigned on one branch only reads as php's NULL with the warning when the branch did not run, whatever type the branch stored (int, float, bool, string, array, object): as a return value, a var_dump argument, a method and a function argument, an interpolation and a comparison. The frame slot of such a read held uninitialized storage, which printed as a garbage int (`elephc-08wi`).
#[test]
fn test_maybe_undefined_local_of_every_type_reads_null_with_a_warning() {
    let source = r####"<?php
class Pt { public $x = 1; }
class Reg { public function same($v) { return $v; } }
function id($v) { return $v; }
function ret(int $n, $kind) {
    if ($n > 5) {
        $i = 1; $f = 1.5; $b = true; $s = "s"; $a = [1, 2]; $o = new Pt;
    }
    return match ($kind) { 'i' => $i, 'f' => $f, 'b' => $b, 's' => $s, 'a' => $a, 'o' => $o };
}
function boxed(int $n) {
    if ($n > 5) { $i = 1; $f = 1.5; $b = true; $s = "s"; $a = [1, 2]; $o = new Pt; }
    var_dump($i, $f, $b, $s, $a, $o);
}
$r = new Reg;
$n = $argc;
foreach (['i', 'f', 'b', 's', 'a', 'o'] as $kind) {
    var_dump(ret($n, $kind));
    var_dump(ret($n + 9, $kind) === ret($n + 9, $kind));
}
boxed($n);
boxed($n + 9);
if ($n > 5) { $q = 1; }
var_dump($q);
var_dump($r->same($q));
var_dump(id($q));
var_dump(gettype($q), $q === null, is_null($q));
if ($n > 5) { $t = "str"; }
echo "[", $t, "]\n";
echo "v={$t} w=$t\n";
var_dump($t . "x", (string)$t, strlen($t ?? ''));
$q = 4;
var_dump($q);"####;
    checked(
        source,
        r####"Warning: Undefined variable $i on line 9
NULL
bool(true)
Warning: Undefined variable $f on line 9
NULL
bool(true)
Warning: Undefined variable $b on line 9
NULL
bool(true)
Warning: Undefined variable $s on line 9
NULL
bool(true)
Warning: Undefined variable $a on line 9
NULL
bool(true)
Warning: Undefined variable $o on line 9
NULL
bool(false)
Warning: Undefined variable $i on line 13
Warning: Undefined variable $f on line 13
Warning: Undefined variable $b on line 13
Warning: Undefined variable $s on line 13
Warning: Undefined variable $a on line 13
Warning: Undefined variable $o on line 13
NULL
NULL
NULL
NULL
NULL
NULL
int(1)
float(1.5)
bool(true)
string(1) "s"
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
object(Pt)#3 (1) {
  ["x"]=>
  int(1)
}
Warning: Undefined variable $q on line 24
NULL
Warning: Undefined variable $q on line 25
NULL
Warning: Undefined variable $q on line 26
NULL
Warning: Undefined variable $q on line 27
Warning: Undefined variable $q on line 27
Warning: Undefined variable $q on line 27
string(4) "NULL"
bool(true)
bool(true)
[
Warning: Undefined variable $t on line 29
]
Warning: Undefined variable $t on line 30
Warning: Undefined variable $t on line 30
v= w=
Warning: Undefined variable $t on line 31
Warning: Undefined variable $t on line 31
string(1) "x"
string(0) ""
int(0)
int(4)"####,
        r####"NULL
bool(true)
NULL
bool(true)
NULL
bool(true)
NULL
bool(true)
NULL
bool(true)
NULL
bool(false)
NULL
NULL
NULL
NULL
NULL
NULL
int(1)
float(1.5)
bool(true)
string(1) "s"
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
object(Pt)#3 (1) {
  ["x"]=>
  int(1)
}
NULL
NULL
NULL
string(4) "NULL"
bool(true)
bool(true)
[]
v= w=
string(1) "x"
string(0) ""
int(0)
int(4)
"####,
    );
}

/// `unset()` of a typed local, on every path or on one branch, makes the next read warn and yield NULL; a store afterwards defines the variable again.
#[test]
fn test_unset_local_reads_as_undefined_for_every_type() {
    let source = r####"<?php
class Pt { public $x = 1; }
function cond_unset(int $n, $kind) {
    $i = 1; $f = 1.5; $b = true; $s = "s"; $a = [1, 2]; $o = new Pt;
    if ($n > 5) { unset($i, $f, $b, $s, $a, $o); }
    return match ($kind) { 'i' => $i, 'f' => $f, 'b' => $b, 's' => $s, 'a' => $a, 'o' => $o };
}
foreach (['i', 'f', 'b', 's', 'a', 'o'] as $kind) {
    var_dump(cond_unset($argc, $kind));
    var_dump(cond_unset($argc + 9, $kind));
}
$i = 1; $s = "s"; $f = 2.5;
unset($i, $s, $f);
var_dump($i, $s, $f);
$i = 3;
var_dump($i);
for ($k = 0; $k < 2; $k++) { $loop = $k; unset($loop); }
var_dump($loop);
function again(int $n) {
    $v = 10;
    unset($v);
    if ($n > 0) { $v = 20; }
    return $v;
}
var_dump(again(0), again(1));"####;
    checked(
        source,
        r####"int(1)
Warning: Undefined variable $i on line 6
NULL
float(1.5)
Warning: Undefined variable $f on line 6
NULL
bool(true)
Warning: Undefined variable $b on line 6
NULL
string(1) "s"
Warning: Undefined variable $s on line 6
NULL
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
Warning: Undefined variable $a on line 6
NULL
object(Pt)#1 (1) {
  ["x"]=>
  int(1)
}
Warning: Undefined variable $o on line 6
NULL
Warning: Undefined variable $i on line 14
Warning: Undefined variable $s on line 14
Warning: Undefined variable $f on line 14
NULL
NULL
NULL
int(3)
Warning: Undefined variable $loop on line 18
NULL
Warning: Undefined variable $v on line 23
NULL
int(20)"####,
        r####"int(1)
NULL
float(1.5)
NULL
bool(true)
NULL
string(1) "s"
NULL
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
NULL
object(Pt)#1 (1) {
  ["x"]=>
  int(1)
}
NULL
NULL
NULL
NULL
int(3)
NULL
NULL
int(20)
"####,
    );
}

/// The same warning and NULL through a method argument, a method return, by-value and by-reference closure captures, a generator, a static, switch, try and loop joins, `+=`, `++` and `.=`.
#[test]
fn test_maybe_undefined_local_in_method_closure_generator_and_loop_contexts() {
    let source = r####"<?php
class Reg {
    public function same($v) { return $v; }
    public function inc(&$v) { $v = 7; }
    public function viaMethod(int $n) { if ($n > 5) { $q = 1; } return $q; }
}
function byref(&$v) { $v = 5; }
function viaClosure(int $n) {
    if ($n > 5) { $q = 1; }
    $by = function () use ($q) { var_dump($q); };
    $by();
    $ref = function () use (&$q) { var_dump($q); };
    $ref();
    $set = function () use (&$q) { $q = 4; };
    $set();
    var_dump($q);
}
function viaGenerator(int $n) {
    if ($n > 5) { $q = 1; }
    yield $q;
    $a = 10;
    yield $a;
    unset($a);
    yield $a;
}
function viaStatic(int $n) {
    static $s;
    if ($n > 5) { $s = 1; }
    return $s;
}
function viaSwitch(int $n) {
    switch ($n) { case 5: $q = 1; break; case 6: $q = 2; break; }
    return $q;
}
function viaTry(int $n) {
    try { if ($n > 0) { throw new Exception("x"); } $q = 1; } catch (Exception $e) { }
    return $q;
}
function viaLoop(int $n) {
    for ($i = 0; $i < $n - 1; $i++) { $q = $i; }
    foreach ([] as $x) { $w = $x; }
    return [$q, $w];
}
$r = new Reg;
$n = $argc;
if ($n > 5) { $q = 1; }
var_dump($r->same($q));
var_dump($r->viaMethod($n));
viaClosure($n);
foreach (viaGenerator($n) as $v) { var_dump($v); }
var_dump(viaStatic($n));
var_dump(viaSwitch($n));
var_dump(viaTry($n));
var_dump(viaLoop($n));
$q += 2; var_dump($q);
if ($n > 5) { $c = 1; }
$c++; var_dump($c);
if ($n > 5) { $m = "a"; }
$m .= "b"; var_dump($m);"####;
    checked(
        source,
        r####"Warning: Undefined variable $q on line 47
NULL
Warning: Undefined variable $q on line 5
NULL
Warning: Undefined variable $q on line 10
NULL
NULL
int(4)
Warning: Undefined variable $q on line 20
NULL
int(10)
Warning: Undefined variable $a on line 24
NULL
NULL
Warning: Undefined variable $q on line 33
NULL
Warning: Undefined variable $q on line 37
NULL
Warning: Undefined variable $q on line 42
Warning: Undefined variable $w on line 42
array(2) {
  [0]=>
  NULL
  [1]=>
  NULL
}
Warning: Undefined variable $q on line 55
int(2)
Warning: Undefined variable $c on line 57
int(1)
Warning: Undefined variable $m on line 59
string(1) "b""####,
        r####"NULL
NULL
NULL
NULL
int(4)
NULL
int(10)
NULL
NULL
NULL
NULL
array(2) {
  [0]=>
  NULL
  [1]=>
  NULL
}
int(2)
int(1)
string(1) "b"
"####,
    );
}

/// A maybe-undefined local destructured by `[$a, $b] = $l`, named by `compact()`, probed by `??`/`isset()`/`empty()` and passed to a by-reference parameter of a function or method (which defines it as NULL without a warning).
#[test]
fn test_maybe_undefined_local_through_list_compact_and_reference_parameters() {
    let source = r####"<?php
class Reg { public function inc(&$v) { $v = 7; } }
function byref(&$v) { $v = 5; }
function t6(int $n, Reg $r) {
    if ($n > 5) { $c = 1; }
    $c++; var_dump($c);
    if ($n > 5) { $m = "a"; }
    $m .= "b"; var_dump($m);
    if ($n > 5) { $l = [1, 2]; }
    [$p1, $p2] = $l; var_dump($p1, $p2);
    if ($n > 5) { $k = 1; }
    var_dump(compact('k'), $k ?? 'dflt', isset($k), empty($k));
    byref($u1); var_dump($u1);
    $r->inc($u2); var_dump($u2);
    if ($n > 5) { $w = 1; }
    byref($w); var_dump($w);
    if ($n > 5) { $z = 1; }
    $r->inc($z); var_dump($z);
}
t6($argc, new Reg);
t6($argc + 9, new Reg);"####;
    checked(
        source,
        r####"Warning: Undefined variable $c on line 6
int(1)
Warning: Undefined variable $m on line 8
string(1) "b"
Warning: Undefined variable $l on line 10
NULL
NULL
Warning: compact(): Undefined variable $k on line 12
array(0) {
}
string(4) "dflt"
bool(false)
bool(true)
int(5)
int(7)
int(5)
int(7)
int(2)
string(2) "ab"
int(1)
int(2)
array(1) {
  ["k"]=>
  int(1)
}
int(1)
bool(true)
bool(false)
int(5)
int(7)
int(5)
int(7)"####,
        r####"int(1)
string(1) "b"
NULL
NULL
array(0) {
}
string(4) "dflt"
bool(false)
bool(true)
int(5)
int(7)
int(5)
int(7)
int(2)
string(2) "ab"
int(1)
int(2)
array(1) {
  ["k"]=>
  int(1)
}
int(1)
bool(true)
bool(false)
int(5)
int(7)
int(5)
int(7)
"####,
    );
}
