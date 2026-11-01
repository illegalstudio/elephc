//! Purpose:
//! Regression tests for method results reached through an untyped or `mixed` receiver.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Such a call dispatches at run time and boxes the callee's result for the `mixed` call site.
//!   The box RETAINED the returned container or object while the call's own reference stayed live,
//!   so every object returned this way leaked and its destructor never ran. The box now takes the
//!   call's reference (`store_method_call_result`).
//! - A boxed value returned under a declared object type is unboxed by the return guard, which
//!   takes its own reference; the box is released afterwards.

use crate::support::*;

/// An object returned by a method called on an untyped or `mixed` receiver is destroyed when its
/// last holder lets go, exactly as for a typed receiver.
#[test]
fn test_mixed_receiver_call_results_are_released() {
    let out = compile_and_run(
        r#"<?php
class It { public function __construct(public string $n) {} public function __destruct() { echo "[~", $this->n, "]"; } }
class Maker { public function make(): It { return new It("m"); } }
function a(mixed $m) { $r = $m->make(); unset($r); echo "|a"; }
function b(Maker $m) { $r = $m->make(); unset($r); echo "|b"; }
function c($m) { $r = $m->make(); unset($r); echo "|c"; }
a(new Maker()); echo "\n";
b(new Maker()); echo "\n";
c(new Maker()); echo "\n";
$x = new Maker(); $mm = [$x][0]; $r = $mm->make(); unset($r); echo "|d\n";
"#,
    );
    assert_eq!(out, "[~m]|a\n[~m]|b\n[~m]|c\n[~m]|d\n");
}

/// A `mixed` call result returned under a declared `Iterator` type, and iterated, is destroyed.
#[test]
fn test_mixed_receiver_results_behind_a_declared_return_type() {
    let out = compile_and_run(
        r#"<?php
class It implements Iterator {
    private int $i = 0;
    public function __construct(private string $n, private array $v) {}
    public function __destruct() { echo "[~it ", $this->n, "]"; }
    public function current(): mixed { return $this->v[$this->i]; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < count($this->v); }
}
class Maker { public function make(): It { return new It("m", [1]); } }
function viaMixed(mixed $m): Iterator { return $m->make(); }
function viaTyped(Maker $m): Iterator { return $m->make(); }
$x = viaTyped(new Maker()); unset($x); echo "|typed\n";
$y = viaMixed(new Maker()); unset($y); echo "|mixed\n";
foreach (viaMixed(new Maker()) as $v) { echo $v; } echo "|foreach\n";
"#,
    );
    assert_eq!(out, "[~it m]|typed\n[~it m]|mixed\n1[~it m]|foreach\n");
}

/// Nested `IteratorAggregate`s over an untyped property: every aggregate and iterator is destroyed
/// in php's order.
#[test]
fn test_nested_iterator_aggregates_destroy_their_iterators() {
    let out = compile_and_run(
        r#"<?php
class It implements Iterator {
    private int $i = 0;
    public function __construct(private string $n, private array $v) {}
    public function __destruct() { echo "[~it ", $this->n, "]"; }
    public function current(): mixed { return $this->v[$this->i]; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < count($this->v); }
}
class Agg implements IteratorAggregate {
    public function __construct(private string $n, private $inner) {}
    public function __destruct() { echo "[~agg ", $this->n, "]"; }
    public function getIterator(): Iterator { return is_array($this->inner) ? new It($this->n, $this->inner) : $this->inner->getIterator(); }
}
foreach (new Agg("outer", new Agg("inner", [1, 2])) as $v) { echo $v; }
echo "\nafter\n";
function run() { foreach (new Agg("f", [3]) as $v) { echo $v; } echo "|end"; }
run();
echo "\ndone\n";
"#,
    );
    assert_eq!(out, "[~agg outer][~agg inner]12[~it inner]\nafter\n[~agg f]3[~it f]|end\ndone\n");
}
