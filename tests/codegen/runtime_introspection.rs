//! Purpose:
//! Integration tests for the runtime introspection builtins `is_countable()` and `getmypid()`
//! (issue #860), compiled and inside `eval()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected string was produced by PHP 8.5 on the same fixture, except the process id,
//!   which is checked by its properties (a positive, stable integer that `eval()` agrees with).

use crate::support::*;

/// Verifies `is_countable()` answers like PHP for scalars, arrays, plain and `Countable`
/// objects (direct, inherited, and through an interface extending `Countable`), the SPL
/// containers, and a closure.
#[test]
fn test_is_countable_matches_php_for_every_value_kind() {
    let out = compile_and_run(
        r#"<?php
interface MyCountable extends Countable {}
class Plain {}
class Counted implements Countable {
    public function count(): int { return 3; }
}
class SubCounted extends Counted {}
class ViaInterface implements MyCountable {
    public function count(): int { return 1; }
}
$values = [
    [], [1, 2], ["a" => 1], 0, 1.5, "abc", null, true, false,
    new Plain(), new Counted(), new SubCounted(), new ViaInterface(),
    new ArrayObject([1, 2]), new ArrayIterator([]), new SplFixedArray(2),
    new SplObjectStorage(), new SplStack(), function () {},
];
foreach ($values as $v) {
    echo is_countable($v) ? "1" : "0";
}
echo "|";
echo is_countable([]) ? "y" : "n";
echo is_countable(new Plain()) ? "y" : "n";
echo is_countable(new Counted()) ? "y" : "n";
echo is_countable("abc") ? "y" : "n";
"#,
    );
    assert_eq!(out, "1110000000111111110|ynyn");
}

/// Verifies `is_countable()` checks the runtime class when the declared type cannot decide it:
/// a `Base` parameter may hold a `Countable` subclass, and an `iterable` may hold an array, a
/// `Countable` iterator, or a generator (which is not countable).
#[test]
fn test_is_countable_checks_the_runtime_class_behind_a_declared_type() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Child extends Base implements Countable {
    public function count(): int { return 2; }
}
function typed_base(Base $b): bool { return is_countable($b); }
function typed_iter(iterable $it): bool { return is_countable($it); }
function typed_countable(Countable $c): bool { return is_countable($c); }
echo typed_base(new Base()) ? "y" : "n";
echo typed_base(new Child()) ? "y" : "n";
echo "|";
echo typed_iter([1]) ? "y" : "n";
echo typed_iter(["k" => 1]) ? "y" : "n";
echo typed_iter(new ArrayIterator([])) ? "y" : "n";
echo typed_iter((function () { yield 1; })()) ? "y" : "n";
echo "|";
echo typed_countable(new Child()) ? "y" : "n";
"#,
    );
    assert_eq!(out, "ny|yyyn|y");
}

/// Verifies `is_countable()` resolves case-insensitively, through the global fallback of a
/// namespaced call, fully qualified, and through a variable function name.
#[test]
fn test_is_countable_case_insensitive_namespaced_and_variable_calls() {
    let out = compile_and_run(
        r#"<?php
namespace App;
class Counted implements \Countable {
    public function count(): int { return 1; }
}
echo IS_COUNTABLE([1]) ? "y" : "n";
echo is_countable(new Counted()) ? "y" : "n";
echo \is_countable(7) ? "y" : "n";
$f = 'is_countable';
echo $f(["a" => 1]) ? "y" : "n";
echo $f("str") ? "y" : "n";
"#,
    );
    assert_eq!(out, "yynyn");
}

/// Verifies `is_countable()` inside `eval()` agrees with PHP for arrays, scalars, compiled and
/// eval-declared classes, the SPL containers, and a dynamic `call_user_func()` dispatch.
#[test]
fn test_is_countable_inside_eval() {
    let out = compile_and_run(
        r#"<?php
class Counted implements Countable {
    public function count(): int { return 3; }
}
class Plain {}
$c = new Counted();
$p = new Plain();
echo eval('return is_countable([1]) ? "y" : "n";');
echo eval('return is_countable(5) ? "y" : "n";');
echo eval('return is_countable($c) ? "y" : "n";');
echo eval('return is_countable($p) ? "y" : "n";');
echo eval('return is_countable(new ArrayObject([])) ? "y" : "n";');
echo eval('class EvalCounted implements Countable { public function count(): int { return 0; } } return is_countable(new EvalCounted()) ? "y" : "n";');
echo eval('return call_user_func("is_countable", ["a" => 1]) ? "y" : "n";');
"#,
    );
    assert_eq!(out, "ynynyyy");
}

/// Verifies `getmypid()` returns a positive integer that is stable across calls, resolves
/// case-insensitively, and agrees with the id `eval()` reports from inside the same process.
///
/// The compiled call reads libc `getpid()` and eval reads Rust's `std::process::id()`, so the
/// agreement checks two independent sources rather than one against itself.
#[test]
fn test_getmypid_returns_the_process_id() {
    let out = compile_and_run(
        r#"<?php
$pid = getmypid();
echo is_int($pid) ? "int" : "notint", "|";
echo $pid > 0 ? "positive" : "bad", "|";
echo $pid === GETMYPID() ? "stable" : "unstable", "|";
echo eval('return getmypid();') === $pid ? "same" : "differ", "|";
echo function_exists("getmypid") ? "known" : "unknown";
"#,
    );
    assert_eq!(out, "int|positive|stable|same|known");
}

/// Verifies both builtins are reachable through runtime-selected string callables, not only
/// through direct calls: a variable call, `call_user_func()`, `call_user_func_array()`,
/// `is_callable()` and `array_map()` with a name only known at run time (it comes from `$argv`,
/// so no compile-time binding can resolve it), spelled in mixed case, plus the same calls inside
/// `eval()`.
///
/// `getmypid()` used to be refused by runtime string dispatch ("Call to undefined function")
/// because its typed target had no runtime-callable wrapper contract. Expected output is
/// PHP 8.5.10's.
#[test]
fn test_runtime_named_calls_reach_getmypid_and_is_countable() {
    let out = compile_and_run(
        r#"<?php
$pidName = $argv[1] ?? "GetMyPid";
$countableName = $argv[2] ?? "IS_COUNTABLE";
$pid = getmypid();
echo $pidName() === $pid ? "call" : "bad", "|";
echo call_user_func($pidName) === $pid ? "cuf" : "bad", "|";
echo call_user_func_array($pidName, []) === $pid ? "cufa" : "bad", "|";
echo is_callable($pidName) ? "callable" : "bad", "|";
$first = getmypid(...);
echo $first() === $pid ? "fcc" : "bad", "\n";
var_dump($countableName([1, 2]), $countableName("text"));
var_dump(call_user_func($countableName, new ArrayObject([])));
echo json_encode(array_map($countableName, [[], 1, new ArrayObject([]), null])), "\n";
echo eval('$n = "getmypid"; return $n();') === $pid ? "eval-call" : "bad", "|";
echo eval('$c = "is_countable"; return $c(7) ? "bad" : "eval-var";'), "\n";
"#,
    );
    assert_eq!(
        out,
        "call|cuf|cufa|callable|fcc\nbool(true)\nbool(false)\nbool(true)\n[true,false,true,false]\neval-call|eval-var\n"
    );
}
