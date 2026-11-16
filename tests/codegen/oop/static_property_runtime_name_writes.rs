//! Purpose:
//! Regressions for static-property stores through a runtime name (`C::$$name = ...`) and for the
//! errors a runtime name or a closed-world scoped constant raises.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Expected output is the `php -n` output of the same program.
//! - The store must give the slot its own reference to a borrowed value, and a copy between two
//!   runtime-named slots of one class must not be mistaken for an in-place write-back.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug};

/// A borrowed object stored through a runtime name outlives its local owner, `C::$$a = C::$$b`
/// copies between two distinct slots (each releasing what it held), and an unknown runtime name
/// throws php's catchable `Error`.
#[test]
fn test_runtime_named_static_store_owns_its_value_and_copies_between_slots() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Foo { public $v; function __construct($v) { $this->v = $v; } function __destruct() { echo "destroy ", $this->v, "\n"; } }
class C { public static ?Foo $x = null; public static ?Foo $y = null; }
function f(string $n, int $v) { $o = new Foo($v); C::$$n = $o; }
f('x', 1);
echo C::$x->v, "\n";
f('y', 2);
$a = 'x'; $b = 'y';
C::$$a = C::$$b;
echo C::$x->v, C::$y->v, "\n";
C::$$b = null;
echo "after y null\n";
echo C::$x->v, "\n";
C::$$a = null;
echo "end\n";
try { $n = 'nope'; $v = C::$$n; } catch (\Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "1\ndestroy 1\n22\nafter y null\n2\ndestroy 2\nend\nError: Access to undeclared static property C::$nope\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A write through an unknown runtime name throws the same catchable `Error` as a read.
#[test]
fn test_runtime_named_static_write_to_unknown_name_is_catchable() {
    let out = compile_and_run(
        r#"<?php
class C { public static int $x = 1; }
$n = 'nope';
try { C::$$n = 3; } catch (\Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo C::$x, "\n";
"#,
    );
    assert_eq!(out, "Error: Access to undeclared static property C::$nope\n1\n");
}
