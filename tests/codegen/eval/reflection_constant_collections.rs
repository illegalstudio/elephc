//! Purpose:
//! Heap regression tests for the constant collections of eval reflectors.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval::reflection_constant_collections` through Rust's
//!   test harness.
//!
//! Key details:
//! - `getConstants()` and `getReflectionConstants()` inside eval inserted their keys and values
//!   with a retaining `array_set` and never released the temporaries, leaking two blocks per
//!   constant and a whole reflector per `ReflectionClassConstant`; the constant value a
//!   reflector stores was never released on the compiled-class path either.
//! - Enum cases are left to `reflection_enum_declaring_class`: their case objects are
//!   process-lifetime singletons, so a fixture that materializes them is never heap-clean.
//! - Fixtures run under heap debug inside a function. Expected output is php 8.5's.

use crate::support::*;

/// Constant maps and constant reflectors of eval-declared and compiled classes are complete
/// and leave the heap clean once the eval fragment ends.
#[test]
fn test_eval_reflection_constant_collections_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class AotConsts { const X = 1; const Y = "two"; }
function probe(): void {
    eval('class EvalConsts { const K = 3; const L = [4, 5]; }
$c = new ReflectionClass("EvalConsts");
echo json_encode($c->getConstants()), "|", count($c->getReflectionConstants()), "|", $c->getReflectionConstant("L")->getName(), "|", json_encode($c->getReflectionConstant("L")->getValue()), "\n";
$a = new ReflectionClass("AotConsts");
echo json_encode($a->getConstants()), "|", count($a->getReflectionConstants()), "|", $a->getReflectionConstant("Y")->getValue(), "\n";');
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "{\"K\":3,\"L\":[4,5]}|2|L|[4,5]\n",
            "{\"X\":1,\"Y\":\"two\"}|2|two\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
