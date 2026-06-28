//! Purpose:
//! Regression tests for method-call ownership of returned/receiver objects:
//! 1. `return $this` from a fluent method must acquire the receiver, so discarding
//!    the result does not drop the refcount to zero and run the destructor while
//!    the original binding is still live (a use-after-free for classes with a
//!    destructor — it crashed with SIGBUS before the fix).
//! 2. A method-call receiver that is itself an owning temporary (a prior chained
//!    call result, or an inline `new X()`) must be released after the call, or its
//!    destructor never runs (a leak).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Liveness is asserted through a static counter incremented in the constructor
//!   and decremented in the destructor, so the tests observe exactly when objects
//!   are freed rather than depending on heap-debug output.

use crate::support::*;

/// A discarded fluent `return $this` call (class with a destructor) must not free
/// the receiver early: the object stays alive and usable, then is destructed once.
#[test]
fn test_return_this_discarded_does_not_free_receiver() {
    let out = compile_and_run(
        r#"<?php
class T {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { T::$alive = T::$alive + 1; }
    public function set(int $x): T { $this->v = $x; return $this; }
    public function __destruct() { T::$alive = T::$alive - 1; }
}
function run(): void {
    $t = new T();
    $t->set(5);
    echo "alive=" . T::$alive . ";v=" . $t->v . "\n";
}
run();
echo "after=" . T::$alive;
"#,
    );
    assert_eq!(out, "alive=1;v=5\nafter=0");
}

/// A fluent chain of `return $this` calls keeps exactly one live object and
/// destructs it exactly once (the acquired intermediates are released).
#[test]
fn test_return_this_chain_balances_refcount() {
    let out = compile_and_run(
        r#"<?php
class T {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { T::$alive = T::$alive + 1; }
    public function set(int $x): T { $this->v = $x; return $this; }
    public function __destruct() { T::$alive = T::$alive - 1; }
}
function run(): void {
    $t = new T();
    $t->set(1)->set(2)->set(3);
    echo "alive=" . T::$alive . ";v=" . $t->v . "\n";
}
run();
echo "after=" . T::$alive;
"#,
    );
    assert_eq!(out, "alive=1;v=3\nafter=0");
}

/// A chain whose receiver is an owning temporary that returns a NEW object each
/// step releases every intermediate, so all objects are destructed (no leak).
#[test]
fn test_chained_owning_receiver_temporaries_are_released() {
    let out = compile_and_run(
        r#"<?php
class N {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { N::$alive = N::$alive + 1; }
    public function make(): N { $o = new N(); $o->v = $this->v + 1; return $o; }
    public function __destruct() { N::$alive = N::$alive - 1; }
}
function run(): void {
    $t = new N();
    $t->make()->make();
    echo "alive=" . N::$alive . "\n";
}
run();
echo "after=" . N::$alive;
"#,
    );
    assert_eq!(out, "alive=1\nafter=0");
}

/// Assigning a fluent `return $this` result keeps the (aliased) object alive while
/// the binding is in scope and frees it exactly once at scope end.
#[test]
fn test_return_this_assigned_result_is_single_owned() {
    let out = compile_and_run(
        r#"<?php
class T {
    public static int $alive = 0;
    public int $v = 0;
    public function __construct() { T::$alive = T::$alive + 1; }
    public function set(int $x): T { $this->v = $x; return $this; }
    public function __destruct() { T::$alive = T::$alive - 1; }
}
function run(): void {
    $t = new T();
    $x = $t->set(9);
    echo "alive=" . T::$alive . ";v=" . $x->v . "\n";
}
run();
echo "after=" . T::$alive;
"#,
    );
    assert_eq!(out, "alive=1;v=9\nafter=0");
}
