//! Purpose:
//! End-to-end tests for SPL iterator helper builtins.
//! Covers iterator_count(), iterator_to_array(), and iterator_apply() over user iterators.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Fixtures assert PHP-visible traversal order, key preservation, callback stopping, and final iterator position.

use crate::support::*;

/// Verifies that iterator count accepts arrays.
#[test]
fn test_iterator_count_accepts_arrays() {
    let out = compile_and_run(
        r#"<?php
echo iterator_count([10, 20, 30]);
echo ":";
echo iterator_count(["a" => 1, "b" => 2]);
"#,
    );
    assert_eq!(out, "3:2");
}

/// Verifies that iterator count rewinds and exhausts iterator.
#[test]
fn test_iterator_count_rewinds_and_exhausts_iterator() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 99; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function pos(): int { return $this->i; }
}
$r = new Range(3);
echo iterator_count($r);
echo ":";
echo $r->pos();
"#,
    );
    assert_eq!(out, "3:3");
}

/// Verifies that iterator count accepts iterator aggregate.
#[test]
fn test_iterator_count_accepts_iterator_aggregate() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 0; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
class Bag implements IteratorAggregate {
    public function getIterator(): Range { return new Range(4); }
}
echo iterator_count(new Bag());
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that iterator count accepts runtime iterable sources.
#[test]
fn test_iterator_count_accepts_runtime_iterable_sources() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 0; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function size(iterable $items): int {
    return iterator_count($items);
}
echo size([1, 2]);
echo ":";
echo size(["a" => 10, "b" => 20, "c" => 30]);
echo ":";
echo size(new Range(4));
"#,
    );
    assert_eq!(out, "2:3:4");
}

/// Verifies that iterator to array accepts arrays.
#[test]
fn test_iterator_to_array_accepts_arrays() {
    let out = compile_and_run(
        r#"<?php
$items = iterator_to_array(["a" => 10, "b" => 20]);
foreach ($items as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "a=10 b=20 ");
}

/// Verifies that iterator to array reindexes associative array without preserving keys.
#[test]
fn test_iterator_to_array_reindexes_associative_array_without_preserving_keys() {
    let out = compile_and_run(
        r#"<?php
$items = iterator_to_array(["a" => 10, "b" => 20], false);
foreach ($items as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "0=10 1=20 ");
}

/// Verifies that iterator to array accepts runtime iterable sources.
#[test]
fn test_iterator_to_array_accepts_runtime_iterable_sources() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i + 7; }
    public function key(): string { return "k" . $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function dump_preserved(iterable $items): void {
    $copy = iterator_to_array($items);
    foreach ($copy as $k => $v) {
        echo $k;
        echo "=";
        echo $v;
        echo " ";
    }
}
function dump_values(iterable $items): void {
    $copy = iterator_to_array($items, false);
    foreach ($copy as $k => $v) {
        echo $k;
        echo "=";
        echo $v;
        echo " ";
    }
}
dump_preserved(["a" => 10, "b" => 20]);
echo "|";
dump_values(["a" => 10, "b" => 20]);
echo "|";
dump_values([3, 4]);
echo "|";
dump_preserved(new Range());
"#,
    );
    assert_eq!(out, "a=10 b=20 |0=10 1=20 |0=3 1=4 |k0=7 k1=8 ");
}

/// Verifies that iterator to array accepts dynamic preserve keys.
#[test]
fn test_iterator_to_array_accepts_dynamic_preserve_keys() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i + 5; }
    public function key(): string { return "k" . $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function dump(iterable $items, bool $preserve): void {
    $copy = iterator_to_array($items, $preserve);
    echo count($copy);
    echo ":";
    foreach ($copy as $k => $v) {
        echo $k;
        echo "=";
        echo $v;
        echo " ";
    }
}
dump(["a" => 10, "b" => 20], true);
echo "|";
dump(["a" => 10, "b" => 20], false);
echo "|";
dump(new Range(), true);
echo "|";
dump(new Range(), false);
"#,
    );
    assert_eq!(out, "2:a=10 b=20 |2:0=10 1=20 |2:k0=5 k1=6 |2:0=5 1=6 ");
}

/// Verifies that iterator to array without preserving keys.
#[test]
fn test_iterator_to_array_without_preserving_keys() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $start, int $end) { $this->i = $start; $this->end = $end; }
    public function rewind(): void {}
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i + 100; }
    public function next(): void { $this->i = $this->i + 1; }
}
$items = iterator_to_array(new Range(10, 13), false);
foreach ($items as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "0=10 1=11 2=12 ");
}

/// Verifies that iterator to array preserves iterator keys.
#[test]
fn test_iterator_to_array_preserves_iterator_keys() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 0; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i + 20; }
    public function key(): mixed { return "k" . $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
$items = iterator_to_array(new Range(3));
foreach ($items as $k => $v) {
    echo $k;
    echo "=";
    echo $v;
    echo " ";
}
"#,
    );
    assert_eq!(out, "k0=20 k1=21 k2=22 ");
}

/// Verifies that iterator apply counts callback invocations.
#[test]
fn test_iterator_apply_counts_callback_invocations() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 0; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function pos(): int { return $this->i; }
}
function tick(): bool {
    echo "x";
    return true;
}
$r = new Range(3);
$count = iterator_apply($r, "tick");
echo ":";
echo $count;
echo ":";
echo $r->pos();
"#,
    );
    assert_eq!(out, "xxx:3:3");
}

/// Verifies that iterator apply stops before next when callback returns false.
#[test]
fn test_iterator_apply_stops_before_next_when_callback_returns_false() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 5; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function pos(): int { return $this->i; }
}
function stop_after_two(): bool {
    static $n = 0;
    $n = $n + 1;
    echo $n;
    return $n < 2;
}
$r = new Range();
$count = iterator_apply($r, "stop_after_two");
echo ":";
echo $count;
echo ":";
echo $r->pos();
"#,
    );
    assert_eq!(out, "12:2:1");
}

/// Verifies that iterator apply passes literal args to callback.
#[test]
fn test_iterator_apply_passes_literal_args_to_callback() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function label(string $prefix): bool {
    echo $prefix;
    return true;
}
echo iterator_apply(new Range(), "label", ["A"]);
"#,
    );
    assert_eq!(out, "AA2");
}

/// Verifies that iterator apply evaluates literal arg array once before loop.
#[test]
fn test_iterator_apply_evaluates_literal_arg_array_once_before_loop() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function make_label(): string {
    echo "!";
    return "A";
}
function label(string $prefix): bool {
    echo $prefix;
    return true;
}
echo iterator_apply(new Range(), "label", [make_label()]);
"#,
    );
    assert_eq!(out, "!AA2");
}

/// Verifies that iterator apply accepts static args for callable without known signature.
#[test]
fn test_iterator_apply_accepts_static_args_for_callable_without_known_signature() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function make_tick(): callable {
    return function(): bool {
        echo "x";
        return true;
    };
}
function make_label(): callable {
    return function(string $prefix): bool {
        echo $prefix;
        return true;
    };
}
echo iterator_apply(new Range(), make_tick());
echo ":";
$cb = make_label();
echo iterator_apply(new Range(), $cb, ["A"]);
"#,
    );
    assert_eq!(out, "xx2:AA2");
}

/// Verifies that iterator apply dynamic args for callable without known signature.
#[test]
fn test_iterator_apply_dynamic_args_for_callable_without_known_signature() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function make_label(): callable {
    return function(string $prefix): bool {
        echo $prefix;
        return true;
    };
}
$args = ["B"];
echo iterator_apply(new Range(), make_label(), $args);
"#,
    );
    assert_eq!(out, "BB2");
}

/// Verifies that iterator apply unknown signature captured callback dynamic args overflow stack.
#[test]
fn test_iterator_apply_unknown_signature_captured_callback_dynamic_args_overflow_stack() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 1; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
$base = 10;
$callbacks = [
    function(
        $a1, $a2, $a3, $a4, $a5,
        $a6, $a7, $a8, $a9, $a10,
        $a11, $a12, $a13, $a14, $a15,
        $a16, $a17, $a18, $a19, $a20
    ) use ($base): bool {
        echo $base + $a1 + $a2 + $a3 + $a4 + $a5
            + $a6 + $a7 + $a8 + $a9 + $a10
            + $a11 + $a12 + $a13 + $a14 + $a15
            + $a16 + $a17 + $a18 + $a19 + $a20;
        return true;
    }
];
$cb = $callbacks[0];
$args = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
         11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
echo ":";
echo iterator_apply(new Range(), $cb, $args);
"#,
    );
    assert_eq!(out, ":2201");
}

/// Verifies that iterator apply dynamic assoc args for returned callable signature.
#[test]
fn test_iterator_apply_dynamic_assoc_args_for_returned_callable_signature() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function make_label(): callable {
    return function(string $name): bool {
        echo $name;
        return true;
    };
}
$args = ["name" => "N"];
echo iterator_apply(new Range(), make_label(), $args);
"#,
    );
    assert_eq!(out, "NN2");
}

/// Verifies that iterator apply dynamic assoc args for returned untyped callable signature.
#[test]
fn test_iterator_apply_dynamic_assoc_args_for_returned_untyped_callable_signature() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function make_label(): callable {
    return function($left, $right): bool {
        echo ($left * 10) + $right;
        return true;
    };
}
$args = ["right" => 2, "left" => 1];
echo iterator_apply(new Range(), make_label(), $args);
"#,
    );
    assert_eq!(out, "12122");
}

/// Verifies that iterator apply dynamic assoc args for callable without static signature.
#[test]
fn test_iterator_apply_dynamic_assoc_args_for_callable_without_static_signature() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
$callbacks = [
    function($left, $right): bool {
        echo ($left * 10) + $right;
        return true;
    },
    function($right, $left): bool {
        echo ($right * 100) + $left;
        return true;
    }
];
$idx = 0;
$cb = $callbacks[$idx];
$args = ["right" => 2, "left" => 1];
echo iterator_apply(new Range(), $cb, $args);
"#,
    );
    assert_eq!(out, "12122");
}

/// Verifies that iterator apply dynamic assoc unknown signature uses string truthiness.
#[test]
fn test_iterator_apply_dynamic_assoc_unknown_signature_uses_string_truthiness() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
$callbacks = [
    function($left, $right): string {
        echo ($left * 10) + $right;
        return "";
    },
    function($right, $left): string {
        echo ($right * 100) + $left;
        return "";
    }
];
$idx = 0;
$cb = $callbacks[$idx];
$args = ["right" => 2, "left" => 1];
echo iterator_apply(new Range(), $cb, $args);
"#,
    );
    assert_eq!(out, "121");
}

/// Verifies that iterator apply dynamic indexed unknown signature uses string truthiness.
#[test]
fn test_iterator_apply_dynamic_indexed_unknown_signature_uses_string_truthiness() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
$callbacks = [
    function($label): string {
        echo $label;
        return "";
    },
];
$idx = 0;
$cb = $callbacks[$idx];
$args = ["X"];
echo iterator_apply(new Range(), $cb, $args);
"#,
    );
    assert_eq!(out, "X1");
}

/// Verifies that iterator apply dynamic assoc args for known signature.
#[test]
fn test_iterator_apply_dynamic_assoc_args_for_known_signature() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function label_tick(string $label): bool {
    echo $label;
    return true;
}
$args = ["label" => "L"];
echo iterator_apply(new Range(), "label_tick", $args);
"#,
    );
    assert_eq!(out, "LL2");
}

/// Verifies that iterator apply dynamic string callback without args.
#[test]
fn test_iterator_apply_dynamic_string_callback_without_args() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function tick_name(): bool {
    echo "x";
    return true;
}
$callback = "TICK_NAME";
echo iterator_apply(new Range(), $callback);
"#,
    );
    assert_eq!(out, "xx2");
}

/// Verifies that iterator apply dynamic string callback assoc args.
#[test]
fn test_iterator_apply_dynamic_string_callback_assoc_args() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function label_name(string $label): bool {
    echo $label;
    return true;
}
$callback = "label_name";
$args = ["label" => "S"];
echo iterator_apply(new Range(), $callback, $args);
"#,
    );
    assert_eq!(out, "SS2");
}

/// Verifies that iterator apply dynamic string builtin callback assoc args.
#[test]
fn test_iterator_apply_dynamic_string_builtin_callback_assoc_args() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
$callback = "strlen";
$args = ["string" => "x"];
echo iterator_apply(new Range(), $callback, $args);
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies that iterator apply dynamic args for by ref callback use temp cells.
#[test]
fn test_iterator_apply_dynamic_args_for_by_ref_callback_use_temp_cells() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function bump(&$n): bool {
    $n = $n + 1;
    echo $n;
    return true;
}
$value = 5;
$args = [$value];
echo iterator_apply(new Range(), "bump", $args);
echo ":";
echo $value;
"#,
    );
    assert_eq!(out, "662:5");
}

/// Verifies that iterator apply dynamic assoc args for variadic callback.
#[test]
fn test_iterator_apply_dynamic_assoc_args_for_variadic_callback() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function label_tick(string $label, ...$rest): bool {
    echo $label;
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
    return true;
}
$args = ["label" => "L", "suffix" => "!"];
echo iterator_apply(new Range(), "label_tick", $args);
"#,
    );
    assert_eq!(out, "Lsuffix=!;Lsuffix=!;2");
}

/// Verifies that iterator apply first class dynamic assoc args for variadic callback.
#[test]
fn test_iterator_apply_first_class_dynamic_assoc_args_for_variadic_callback() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 1; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function label_tick(string $label, ...$rest): bool {
    echo $label;
    foreach ($rest as $key => $value) {
        echo $key . "=" . $value . ";";
    }
    return true;
}
$args = ["label" => "L", "suffix" => "!"];
$cb = label_tick(...);
echo iterator_apply(new Range(), $cb, $args);
echo "|";
echo iterator_apply(new Range(), label_tick(...), $args);
"#,
    );
    assert_eq!(out, "Lsuffix=!;1|Lsuffix=!;1");
}

/// Verifies that iterator apply accepts traversable typed source and dynamic args.
#[test]
fn test_iterator_apply_accepts_traversable_typed_source_and_dynamic_args() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $i;
    public function __construct() { $this->i = 0; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
function label(string $prefix): bool {
    echo $prefix;
    return true;
}
function run_traversable(Traversable $items, array $args): int {
    return iterator_apply($items, "label", $args);
}
function run_iterable(iterable $items, array $args): int {
    return iterator_apply($items, "label", $args);
}
$args = ["B"];
echo run_traversable(new Range(), $args);
echo ":";
echo run_iterable(new Range(), $args);
"#,
    );
    assert_eq!(out, "BB2:BB2");
}
