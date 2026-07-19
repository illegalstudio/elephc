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

/// Verifies AOT function callables write back by-reference args before catchable throws.
#[test]
fn test_eval_aot_function_callables_write_back_by_ref_args_before_throw() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_throw_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    throw new Exception("aot-function");
}

echo eval('$string = "eval_aot_throw_ref_add";
$a = "2";
try {
    $string($a, 3);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($a) . ":" . $a . "|";
}

$first = eval_aot_throw_ref_add(...);
$b = "4";
try {
    $first($b, 5);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($b) . ":" . $b . "|";
}

$closure = Closure::fromCallable("eval_aot_throw_ref_add");
$c = "6";
try {
    $closure($c, 7);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":" . gettype($c) . ":" . $c . "|";
}

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
        "Exception:aot-function:integer:5|Exception:aot-function:integer:9|Exception:aot-function:integer:13|Exception:aot-function:integer:17"
    );
}

/// Verifies AOT function argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_aot_function_by_ref_arg_prep_fatal_cleans_up_stack() {
    let cases = [
        (
            "string callable",
            r#"<?php
class EvalAotFunctionStringPrepFatalNeed {}
function eval_aot_function_string_prep_fatal_bridge(int &$value, EvalAotFunctionStringPrepFatalNeed $need): int {
    $value = $value + 1;
    return $value;
}

echo eval('$callback = "eval_aot_function_string_prep_fatal_bridge";
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
        ),
        (
            "first-class callable",
            r#"<?php
class EvalAotFunctionFirstPrepFatalNeed {}
function eval_aot_function_first_prep_fatal_bridge(int &$value, EvalAotFunctionFirstPrepFatalNeed $need): int {
    $value = $value + 1;
    return $value;
}

echo eval('$callback = eval_aot_function_first_prep_fatal_bridge(...);
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
        ),
        (
            "Closure::fromCallable",
            r#"<?php
class EvalAotFunctionClosurePrepFatalNeed {}
function eval_aot_function_closure_prep_fatal_bridge(int &$value, EvalAotFunctionClosurePrepFatalNeed $need): int {
    $value = $value + 1;
    return $value;
}

echo eval('$callback = Closure::fromCallable("eval_aot_function_closure_prep_fatal_bridge");
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
        ),
        (
            "call_user_func_array",
            r#"<?php
class EvalAotFunctionArrayPrepFatalNeed {}
function eval_aot_function_array_prep_fatal_bridge(int &$value, EvalAotFunctionArrayPrepFatalNeed $need): int {
    $value = $value + 1;
    return $value;
}

echo eval('$callback = Closure::fromCallable("eval_aot_function_array_prep_fatal_bridge");
$value = "2";
call_user_func_array($callback, [&$value, 123]);
echo "bad";');
"#,
        ),
    ];

    for (label, source) in cases {
        let out = compile_and_run_capture(source);
        assert!(
            !out.success,
            "{label}: expected eval runtime fatal, stdout={:?} stderr={}",
            out.stdout, out.stderr
        );
        assert_eq!(out.stdout, "", "{label}: unexpected stdout");
        assert!(
            out.stderr.contains("Fatal error: eval() runtime failed"),
            "{label}: stderr did not contain eval runtime fatal diagnostic: {}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
            "{label}: stderr leaked a Rust panic: {}",
            out.stderr
        );
    }
}

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

/// Verifies AOT instance method argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_aot_instance_method_by_ref_arg_prep_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotInstancePrepFatalNeed {}
class EvalAotInstancePrepFatalBridge {
    public function bump(int &$value, EvalAotInstancePrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$box = new EvalAotInstancePrepFatalBridge();
$callback = [$box, "bump"];
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
    );

    assert!(
        !out.success,
        "expected eval runtime fatal, stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
        "stderr leaked a Rust panic: {}",
        out.stderr
    );
}

/// Verifies AOT static method argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_aot_static_method_by_ref_arg_prep_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticPrepFatalNeed {}
class EvalAotStaticPrepFatalBridge {
    public static function bump(int &$value, EvalAotStaticPrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = "EvalAotStaticPrepFatalBridge::bump";
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
    );

    assert!(
        !out.success,
        "expected eval runtime fatal, stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
        "stderr leaked a Rust panic: {}",
        out.stderr
    );
}

/// Verifies AOT static method callable variants restore the eval bridge frame on prep fatal.
#[test]
fn test_eval_aot_static_method_callable_variants_by_ref_arg_prep_fatal_clean_up_stack() {
    let cases = [
        (
            "array callable",
            r#"<?php
class EvalAotStaticVariantArrayPrepFatalNeed {}
class EvalAotStaticVariantArrayPrepFatalBridge {
    public static function bump(int &$value, EvalAotStaticVariantArrayPrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = ["EvalAotStaticVariantArrayPrepFatalBridge", "bump"];
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
        ),
        (
            "first-class callable",
            r#"<?php
class EvalAotStaticVariantFirstClassPrepFatalNeed {}
class EvalAotStaticVariantFirstClassPrepFatalBridge {
    public static function bump(int &$value, EvalAotStaticVariantFirstClassPrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = EvalAotStaticVariantFirstClassPrepFatalBridge::bump(...);
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
        ),
        (
            "Closure::fromCallable array",
            r#"<?php
class EvalAotStaticVariantFromCallablePrepFatalNeed {}
class EvalAotStaticVariantFromCallablePrepFatalBridge {
    public static function bump(int &$value, EvalAotStaticVariantFromCallablePrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = Closure::fromCallable(["EvalAotStaticVariantFromCallablePrepFatalBridge", "bump"]);
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
        ),
    ];

    for (label, source) in cases {
        let out = compile_and_run_capture(source);
        assert!(
            !out.success,
            "{label}: expected eval runtime fatal, stdout={:?} stderr={}",
            out.stdout, out.stderr
        );
        assert_eq!(out.stdout, "", "{label}: unexpected stdout");
        assert!(
            out.stderr.contains("Fatal error: eval() runtime failed"),
            "{label}: stderr did not contain eval runtime fatal diagnostic: {}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
            "{label}: stderr leaked a Rust panic: {}",
            out.stderr
        );
    }
}

/// Verifies named AOT callable by-ref argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_aot_callable_named_ref_arg_prep_fatal_cleans_up_stack() {
    let cases = [
        (
            "instance array callable",
            r#"<?php
class EvalAotNamedRefArrayFatalNeed {}
class EvalAotNamedRefArrayFatalBridge {
    public function bump(int &$value, EvalAotNamedRefArrayFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$box = new EvalAotNamedRefArrayFatalBridge();
$callback = [$box, "bump"];
$value = "2";
$callback(value: $value, need: 123);
echo "bad";');
"#,
        ),
        (
            "first-class instance callable",
            r#"<?php
class EvalAotNamedRefFirstFatalNeed {}
class EvalAotNamedRefFirstFatalBridge {
    public function bump(int &$value, EvalAotNamedRefFirstFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$box = new EvalAotNamedRefFirstFatalBridge();
$callback = $box->bump(...);
$value = "2";
$callback(need: 123, value: $value);
echo "bad";');
"#,
        ),
        (
            "Closure::fromCallable instance",
            r#"<?php
class EvalAotNamedRefClosureFatalNeed {}
class EvalAotNamedRefClosureFatalBridge {
    public function bump(int &$value, EvalAotNamedRefClosureFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$box = new EvalAotNamedRefClosureFatalBridge();
$callback = Closure::fromCallable([$box, "bump"]);
$value = "2";
$callback(value: $value, need: 123);
echo "bad";');
"#,
        ),
        (
            "static string callable",
            r#"<?php
class EvalAotNamedRefStringFatalNeed {}
class EvalAotNamedRefStringFatalBridge {
    public static function bump(int &$value, EvalAotNamedRefStringFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = "EvalAotNamedRefStringFatalBridge::bump";
$value = "2";
$callback(need: 123, value: $value);
echo "bad";');
"#,
        ),
        (
            "first-class static callable",
            r#"<?php
class EvalAotNamedRefStaticFatalNeed {}
class EvalAotNamedRefStaticFatalBridge {
    public static function bump(int &$value, EvalAotNamedRefStaticFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = EvalAotNamedRefStaticFatalBridge::bump(...);
$value = "2";
$callback(value: $value, need: 123);
echo "bad";');
"#,
        ),
        (
            "invokable object callable",
            r#"<?php
class EvalAotNamedRefInvokeFatalNeed {}
class EvalAotNamedRefInvokeFatalBridge {
    public function __invoke(int &$value, EvalAotNamedRefInvokeFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$callback = new EvalAotNamedRefInvokeFatalBridge();
$value = "2";
$callback(need: 123, value: $value);
echo "bad";');
"#,
        ),
    ];

    for (label, source) in cases {
        let out = compile_and_run_capture(source);
        assert!(
            !out.success,
            "{label}: expected eval runtime fatal, stdout={:?} stderr={}",
            out.stdout, out.stderr
        );
        assert_eq!(out.stdout, "", "{label}: unexpected stdout");
        assert!(
            out.stderr.contains("Fatal error: eval() runtime failed"),
            "{label}: stderr did not contain eval runtime fatal diagnostic: {}",
            out.stderr
        );
        assert!(
            !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
            "{label}: stderr leaked a Rust panic: {}",
            out.stderr
        );
    }
}

/// Verifies AOT first-class method argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_aot_first_class_method_by_ref_arg_prep_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotFirstClassPrepFatalNeed {}
class EvalAotFirstClassPrepFatalBridge {
    public function bump(int &$value, EvalAotFirstClassPrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$box = new EvalAotFirstClassPrepFatalBridge();
$callback = $box->bump(...);
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
    );

    assert!(
        !out.success,
        "expected eval runtime fatal, stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
        "stderr leaked a Rust panic: {}",
        out.stderr
    );
}

/// Verifies AOT `Closure::fromCallable()` method argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_aot_closure_from_callable_method_by_ref_arg_prep_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotFromCallablePrepFatalNeed {}
class EvalAotFromCallablePrepFatalBridge {
    public function bump(int &$value, EvalAotFromCallablePrepFatalNeed $need): int {
        $value = $value + 1;
        return $value;
    }
}

echo eval('$box = new EvalAotFromCallablePrepFatalBridge();
$callback = Closure::fromCallable([$box, "bump"]);
$value = "2";
$callback($value, 123);
echo "bad";');
"#,
    );

    assert!(
        !out.success,
        "expected eval runtime fatal, stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
        "stderr leaked a Rust panic: {}",
        out.stderr
    );
}

/// Verifies first-class ref-like builtin fatals restore the eval bridge frame.
#[test]
fn test_eval_first_class_ref_like_builtin_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('$items = [1];
$push = array_push(...);
$push($items);
echo "bad";');
"#,
    );

    assert!(
        !out.success,
        "expected eval runtime fatal, stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
        "stderr leaked a Rust panic: {}",
        out.stderr
    );
}

/// Verifies `Closure::fromCallable()` ref-like builtin fatals restore the eval bridge frame.
#[test]
fn test_eval_closure_from_callable_ref_like_builtin_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('$items = [1];
$push = Closure::fromCallable("array_push");
$push($items);
echo "bad";');
"#,
    );

    assert!(
        !out.success,
        "expected eval runtime fatal, stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("panicked at") && !out.stderr.contains("thread '"),
        "stderr leaked a Rust panic: {}",
        out.stderr
    );
}
