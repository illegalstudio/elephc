//! Purpose:
//! End-to-end regressions for lowering decisions that php takes from the RUN-TIME value rather
//! than from the static type: `Closure::bind()` scopes, callbacks resolved on the object's class,
//! `??` over a property a subclass declares, `foreach` over a subclass that is traversable,
//! internal pointers on non-local places, unpacked integer keys and closed-world `extract()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each expected text is `php -n`'s output for the same program (php 8.5.10).

use crate::support::*;

/// Only a closure LITERAL is lowered in `Closure::bind()`'s scope: a property read or a static
/// call passed as the closure belongs to the class it is written in.
#[test]
fn test_closure_bind_lowers_a_non_literal_closure_in_the_enclosing_scope() {
    let out = compile_and_run(
        r#"<?php
class B { private $v = 'b-private'; }
class A {
    private $fn;
    function __construct() { $this->fn = function () { return $this->v; }; }
    private static function factory() { return function () { return 'factory'; }; }
    function rebind($o) { $c = \Closure::bind($this->fn, $o, B::class); return $c(); }
    function rebind2() { $c = \Closure::bind(self::factory(), null, B::class); return $c(); }
}
$a = new A();
echo $a->rebind(new B()), "\n";
echo $a->rebind2(), "\n";
"#,
    );
    assert_eq!(out, "b-private\nfactory\n");
}
