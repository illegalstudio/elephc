//! Purpose:
//! End-to-end regressions for eval callable by-reference writeback on error paths.
//! Covers callable values crossing eval into generated/AOT and eval-declared methods.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_callable_ref_errors` through Rust's test harness.
//!
//! Key details:
//! - Fixtures verify caller-side by-reference values are written back before a
//!   method callable's catchable throw is returned through the eval bridge.

use crate::support::{compile_and_run, compile_and_run_capture};

/// Verifies AOT method callable by-reference args write back before catchable throws.
#[test]
fn test_eval_aot_method_callables_write_back_by_ref_args_before_throw() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotThrowCallableBridge {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        throw new Exception("aot-instance");
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        throw new Exception("aot-static");
    }
}

echo eval('$box = new EvalAotThrowCallableBridge();

$array = [$box, "bump"];
$a = "2";
try {
    $array($a, 3);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($a) . ":" . $a . "|";
}

$string = "EvalAotThrowCallableBridge::add";
$b = "4";
try {
    $string($b, 5);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($b) . ":" . $b . "|";
}

$first = $box->bump(...);
$c = "6";
try {
    $first($c, 7);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($c) . ":" . $c . "|";
}

$closure = Closure::fromCallable(["EvalAotThrowCallableBridge", "add"]);
$d = "8";
try {
    call_user_func_array($closure, [&$d, 9]);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($d) . ":" . $d;
}');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Exception:aot-instance:integer:15|Exception:aot-static:integer:9|Exception:aot-instance:integer:23|Exception:aot-static:integer:17"
    );
}

/// Verifies eval-declared method callable by-reference args write back before catchable throws.
#[test]
fn test_eval_declared_method_callables_write_back_by_ref_args_before_throw() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalDeclaredThrowCallableBridge {
    public int $base = 20;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        throw new Exception("eval-instance");
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        throw new Exception("eval-static");
    }
}

$box = new EvalDeclaredThrowCallableBridge();

$array = [$box, "bump"];
$a = "2";
try {
    $array($a, 3);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($a) . ":" . $a . "|";
}

$string = "EvalDeclaredThrowCallableBridge::add";
$b = "4";
try {
    $string($b, 5);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($b) . ":" . $b . "|";
}

$first = $box->bump(...);
$c = "6";
try {
    $first($c, 7);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($c) . ":" . $c . "|";
}

$closure = Closure::fromCallable(["EvalDeclaredThrowCallableBridge", "add"]);
$d = "8";
try {
    call_user_func_array($closure, [&$d, 9]);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($d) . ":" . $d;
}');
"#,
    );

    assert_eq!(
        out,
        "Exception:eval-instance:integer:25|Exception:eval-static:integer:9|Exception:eval-instance:integer:33|Exception:eval-static:integer:17"
    );
}
