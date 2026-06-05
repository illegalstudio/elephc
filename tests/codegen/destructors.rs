//! Purpose:
//! End-to-end tests for PHP `__destruct`: the compiler invokes a class's
//! destructor when an object's refcount reaches zero, before its storage is
//! released, across scope exit, overwrite, `unset`, program end, inheritance,
//! container release, and the self-reference re-entrancy guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Destructors run inside the object-free path, so these fixtures assert the
//!   exact interleaving of destructor output with surrounding `echo`s. Ordering
//!   among siblings released together reflects the codegen cleanup order and is
//!   asserted as produced.

use crate::support::*;

/// A destructor fires at function scope exit, before control returns to the
/// caller's following statement.
#[test]
fn test_destruct_on_scope_exit() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __destruct() {
        echo "destroyed\n";
    }
}
function run() {
    $x = new Logger();
    echo "inside\n";
}
run();
echo "after\n";
"#,
    );
    assert_eq!(out, "inside\ndestroyed\nafter\n");
}

/// A top-level object's destructor runs at program end, after main's body.
#[test]
fn test_destruct_at_program_end() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __destruct() { echo "bye\n"; }
}
$x = new Logger();
echo "main\n";
"#,
    );
    assert_eq!(out, "main\nbye\n");
}

/// `unset($x)` releasing the last reference runs the destructor immediately.
#[test]
fn test_destruct_on_unset() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __destruct() { echo "gone\n"; }
}
$x = new Logger();
echo "a\n";
unset($x);
echo "b\n";
"#,
    );
    assert_eq!(out, "a\ngone\nb\n");
}

/// Overwriting a variable releases the previous object (running its destructor),
/// and the destructor can read `$this`'s properties.
#[test]
fn test_destruct_on_overwrite_reads_this() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    private string $tag;
    public function __construct(string $t) { $this->tag = $t; }
    public function __destruct() { echo "drop:" . $this->tag . "\n"; }
}
$x = new Logger("first");
$x = new Logger("second");
echo "end\n";
"#,
    );
    assert_eq!(out, "drop:first\nend\ndrop:second\n");
}

/// A subclass with no destructor inherits its parent's `__destruct`, dispatched
/// to the implementing ancestor's method.
#[test]
fn test_destruct_inherited_from_parent() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __destruct() { echo "base-dtor\n"; }
}
class Derived extends Base {
}
function run() { $d = new Derived(); }
run();
echo "done\n";
"#,
    );
    assert_eq!(out, "base-dtor\ndone\n");
}

/// An overriding subclass destructor runs instead of the parent's.
#[test]
fn test_destruct_override() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function __destruct() { echo "base\n"; }
}
class Derived extends Base {
    public function __destruct() { echo "derived\n"; }
}
function run() { $d = new Derived(); }
run();
echo "end\n";
"#,
    );
    assert_eq!(out, "derived\nend\n");
}

/// Objects held in an array are destructed when the array is released.
#[test]
fn test_destruct_objects_in_array() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public function __destruct() { echo "free "; }
}
function run() {
    $arr = [new Item(), new Item()];
    echo "built ";
}
run();
echo "done";
"#,
    );
    assert_eq!(out, "built free free done");
}

/// A class without `__destruct` is unaffected (no spurious calls, no crash), even
/// alongside a class that defines one.
#[test]
fn test_no_destruct_is_noop() {
    let out = compile_and_run(
        r#"<?php
class Plain {
    public int $v;
    public function __construct() { $this->v = 7; }
}
class Loud {
    public function __destruct() { echo "loud "; }
}
function run() {
    $p = new Plain();
    $l = new Loud();
    echo $p->v . " ";
}
run();
echo "end";
"#,
    );
    assert_eq!(out, "7 loud end");
}

/// The re-entrancy guard: a destructor that takes a local copy of `$this` (a
/// balanced incref then scope-exit decref) must not re-enter the free path or
/// double-run. The destructor runs exactly once.
#[test]
fn test_destruct_self_reference_guard() {
    let out = compile_and_run(
        r#"<?php
class Tricky {
    public function __destruct() {
        $tmp = $this;
        echo "x";
    }
}
function run() { $t = new Tricky(); }
run();
echo "|ok";
"#,
    );
    assert_eq!(out, "x|ok");
}

/// A destructor that releases a heap-backed property (a string) before the
/// object's own storage is freed runs cleanly.
#[test]
fn test_destruct_with_heap_property() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    private array $items;
    public function __construct() { $this->items = ["a", "b", "c"]; }
    public function __destruct() { echo "count:" . count($this->items); }
}
function run() { $h = new Holder(); }
run();
echo "|fin";
"#,
    );
    assert_eq!(out, "count:3|fin");
}
