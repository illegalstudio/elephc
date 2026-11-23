//! Purpose:
//! Regression tests for a by-reference `foreach` over an `iterable`: an array binds its elements by
//! reference, an object raises php's run-time `Error`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The checker used to refuse `foreach ($items as &$v)` over an `iterable` that php runs. php
//!   decides by the value: an array binds by reference (on a COPY of the caller's array, since
//!   arrays pass by value) and a `Traversable` throws "An iterator cannot be used with foreach by
//!   reference" when the loop starts, even for an iterator with no elements.
//! - Every expectation is verbatim `php -n` (PHP 8.5) output for the same fixture.
//! - The callee must not touch the caller's array: an `iterable` parameter is not privatized by
//!   default, so a by-reference loop over one takes an owning shadow like an `array` parameter.

use crate::support::compile_and_run_with_heap_debug;

/// Compiles `source`, runs it and asserts php's output and a clean heap.
fn assert_matches_php(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Arrays and hashes bind by reference inside the callee and the caller's value is untouched; an
/// iterator object, empty or not, raises the Error.
#[test]
fn test_by_reference_foreach_over_iterable_matches_php() {
    assert_matches_php(
        r##"<?php
class Items implements Iterator {
    private int $i = 0;
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
class NoItems implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
}
function double_all(iterable $items): iterable {
    foreach ($items as $key => &$value) { $value = $value * 2; }
    unset($value);
    return $items;
}
function attempt(iterable $items): void {
    try {
        echo json_encode(double_all($items)), "\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
attempt([1, 2, 3]);
attempt(['a' => 4, 'b' => 5]);
attempt([]);
attempt(new Items());
attempt(new NoItems());
$list = [7, 8];
attempt($list);
echo json_encode($list), "\n";
$map = ['x' => 1];
attempt($map);
echo json_encode($map), "\n";
"##,
        "[2,4,6]\n{\"a\":8,\"b\":10}\n[]\nError: An iterator cannot be used with foreach by reference\nError: An iterator cannot be used with foreach by reference\n[14,16]\n[7,8]\n{\"x\":2}\n{\"x\":1}\n",
    );
}

/// An array shared with another variable keeps both values when the callee loops by reference.
#[test]
fn test_by_reference_foreach_over_iterable_splits_a_shared_array() {
    assert_matches_php(
        r##"<?php
function show(iterable $items): void {
    foreach ($items as &$value) { $value = $value + 1; }
    unset($value);
    echo json_encode($items), "\n";
}
$a = [1, 2];
$b = $a;
show($a);
echo json_encode($a), json_encode($b), "\n";
show($b);
echo json_encode($a), json_encode($b), "\n";
"##,
        "[2,3]\n[1,2][1,2]\n[2,3]\n[1,2][1,2]\n",
    );
}

/// A nullable `iterable` narrowed by a null check takes the same run-time decision.
#[test]
fn test_by_reference_foreach_over_narrowed_nullable_iterable_matches_php() {
    assert_matches_php(
        r##"<?php
class NoItems implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
}
function run(?iterable $items): void {
    if ($items === null) { echo "null\n"; return; }
    foreach ($items as &$value) { $value = 1; }
    unset($value);
    echo "done\n";
}
function attempt(?iterable $items): void {
    try { run($items); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
attempt(new NoItems());
attempt(null);
attempt([3]);
"##,
        "Error: An iterator cannot be used with foreach by reference\nnull\ndone\n",
    );
}
