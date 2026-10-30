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

/// The object a narrowed receiver's call returns is destructed like any other: a method the
/// receiver's static interface does not declare dispatches through a boxed `Mixed` result,
/// and that box owns the returned object, whether the result is chained, bound or discarded.
#[test]
fn test_destruct_result_of_narrowed_receiver_call() {
    let out = compile_and_run(
        r#"<?php
interface Response {}
final class Doc implements Response {
    public function __construct(private string $name) {}
    public function __destruct() { echo "destruct ", $this->name, "\n"; }
    public function with(string $step): self { return new self($this->name . '+' . $step); }
    public function name(): string { return $this->name; }
}
function chained(Response $response): string {
    if ($response instanceof Doc) { return $response->with('chained')->name(); }
    return '';
}
function bound(Response $response): string {
    if ($response instanceof Doc) { $next = $response->with('bound'); return $next->name(); }
    return '';
}
function discarded(Response $response): void {
    if ($response instanceof Doc) { $response->with('discarded'); }
    echo "after discard\n";
}
echo chained(new Doc('a')), "\n";
echo bound(new Doc('b')), "\n";
discarded(new Doc('c'));
echo "end\n";
"#,
    );
    // `bound` releases its locals as codegen orders them; PHP destroys `$response` before `$next`.
    assert_eq!(
        out,
        "destruct a+chained\ndestruct a\na+chained\ndestruct b+bound\ndestruct b\nb+bound\n\
         destruct c+discarded\nafter discard\ndestruct c\nend\n"
    );
}

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

/// A method only the narrowed class declares (`if ($r instanceof Doc) { $r->take($x) }`)
/// dispatches on the runtime class, as for a `mixed` receiver, and its arguments are released
/// after the call. They were handed over without the caller's release, so an object passed
/// straight in (a `new` expression, a property read) was never destructed: in a `--web`
/// handler a page's blocks and the connection they held outlived every request.
#[test]
fn test_narrowed_receiver_call_releases_its_arguments() {
    let out = compile_and_run(
        r#"<?php
final class Conn { public function __construct(public int $n) {} public function __destruct() { echo "close {$this->n}\n"; } }
interface Response { public function done(): string; }
final class Doc implements Response {
    public function take(Conn $c): self { return new self(); }
    public function done(): string { return "doc"; }
}
final class Handler {
    public function __construct(private Conn $conn) {}
    public function handle(Response $r, int $n): string {
        if ($r instanceof Doc) {
            return $r->take($this->conn)->done() . " " . $r->take(new Conn($n + 10))->done();
        }
        return $r->done();
    }
    public function direct(Response $r, int $n): string {
        if ($r instanceof Doc) {
            return $r->take(new Conn($n + 20))->done();
        }
        return $r->done();
    }
}
function request(int $n): void {
    $h = new Handler(new Conn($n));
    echo $h->handle(new Doc(), $n), "\n";
    echo $h->direct(new Doc(), $n), "\n";
}
request(1);
request(2);
echo "end\n";
"#,
    );
    assert_eq!(
        out,
        "close 11\ndoc doc\nclose 21\ndoc\nclose 1\nclose 12\ndoc doc\nclose 22\ndoc\nclose 2\nend\n"
    );
}
