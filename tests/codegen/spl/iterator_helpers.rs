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
