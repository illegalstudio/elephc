//! Purpose:
//! End-to-end regressions for runtime eval object construction through AOT classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_constructor` through Rust's test harness.
//!
//! Key details:
//! - Fixtures focus on constructor bridge argument binding and by-reference
//!   writeback for non-variable eval caller targets.

use crate::support::{compile_and_run, compile_and_run_capture};

/// Verifies AOT constructor by-reference args write back to eval lvalue targets.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_writes_back_to_lvalue_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorRefTargetBridge {
    public function __construct(int &$value) {
        $value = $value + 5;
    }
}

class EvalCtorRefTargetBox {
    public int $value = 3;
}

class EvalCtorRefTargetStatic {
    public static int $value = 4;
}

echo eval('$items = ["x" => "1"];
new EvalCtorRefTargetBridge($items["x"]);
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$nested = ["outer" => ["inner" => "2"]];
new EvalCtorRefTargetBridge($nested["outer"]["inner"]);
echo gettype($nested["outer"]["inner"]) . ":" . $nested["outer"]["inner"] . "|";

$box = new EvalCtorRefTargetBox();
new EvalCtorRefTargetBridge($box->value);
echo gettype($box->value) . ":" . $box->value . "|";

EvalCtorRefTargetStatic::$value = "4";
new EvalCtorRefTargetBridge(EvalCtorRefTargetStatic::$value);
return gettype(EvalCtorRefTargetStatic::$value) . ":" . EvalCtorRefTargetStatic::$value;');
"#,
    );

    assert_eq!(out, "integer:6|integer:7|integer:8|integer:9");
}

/// Verifies AOT constructor by-reference args write back through named and unpacked calls.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_named_and_spread_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorNamedRefTargetBridge {
    public function __construct(int &$value, int $delta = 0) {
        $value = $value + $delta;
    }
}

class EvalCtorNamedRefTargetBox {
    public int $value = 4;
}

class EvalCtorNamedRefTargetStatic {
    public static mixed $value = 8;
}

echo eval('$class = "EvalCtorNamedRefTargetBridge";

$value = "2";
new $class(value: $value, delta: 3);
echo gettype($value) . ":" . $value . "|";

$items = ["x" => "4"];
new $class(...["value" => &$items["x"], "delta" => 5]);
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$box = new EvalCtorNamedRefTargetBox();
new $class(delta: 6, value: $box->value);
echo gettype($box->value) . ":" . $box->value . "|";

EvalCtorNamedRefTargetStatic::$value = "8";
new $class(delta: 7, value: EvalCtorNamedRefTargetStatic::$value);
return gettype(EvalCtorNamedRefTargetStatic::$value) . ":" . EvalCtorNamedRefTargetStatic::$value;');
"#,
    );

    assert_eq!(out, "integer:5|integer:9|integer:10|integer:15");
}

/// Verifies ReflectionClass construction uses PHP by-ref semantics for eval and AOT constructors.
#[test]
fn test_eval_reflection_class_constructor_by_ref_matches_php_ref_semantics() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalReflectAotCtorRefBridge {
    public function __construct(int &$value) {
        $value = $value + 5;
    }
}

echo eval('$aotRef = new ReflectionClass("EvalReflectAotCtorRefBridge");
$direct = "1";
$aotRef->newInstance($direct);
echo gettype($direct) . ":" . $direct . "|";

$argsValue = "2";
$aotRef->newInstanceArgs([&$argsValue]);
echo gettype($argsValue) . ":" . $argsValue . "|";

class EvalReflectDeclaredCtorRefBridge {
    public function __construct(int &$value) {
        $value = $value + 7;
    }
}

$evalRef = new ReflectionClass("EvalReflectDeclaredCtorRefBridge");
$evalDirect = "3";
$evalRef->newInstance($evalDirect);
echo gettype($evalDirect) . ":" . $evalDirect . "|";

$evalArgsValue = "4";
$evalRef->newInstanceArgs([&$evalArgsValue]);
return gettype($evalArgsValue) . ":" . $evalArgsValue;');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "string:1|integer:7|string:3|integer:11");
    for warning in [
        "EvalReflectAotCtorRefBridge::__construct(): Argument #1 ($value) must be passed by reference, value given",
        "EvalReflectDeclaredCtorRefBridge::__construct(): Argument #1 ($value) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}

/// Verifies AOT constructor by-reference args write back refcounted string, array, and object values.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_refcounted_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorRefcountedPayload {
    public string $name;
    public function __construct(string $name) {
        $this->name = $name;
    }
}

class EvalCtorStringRefBridge {
    public function __construct(string &$value) {
        $value = $value . "-ctor";
    }
}

class EvalCtorArrayRefBridge {
    public function __construct(array &$items) {
        $items[0] = $items[0] . "-head";
        $items[] = "tail";
    }
}

class EvalCtorObjectRefBridge {
    public function __construct(EvalCtorRefcountedPayload &$box) {
        $box = new EvalCtorRefcountedPayload($box->name . "-ctor");
    }
}

echo eval('$text = "A";
new EvalCtorStringRefBridge($text);
echo $text . "|";

$items = ["B"];
new EvalCtorArrayRefBridge($items);
echo $items[0] . ":" . $items[1] . "|";

$box = new EvalCtorRefcountedPayload("C");
new EvalCtorObjectRefBridge($box);
return $box->name;');
"#,
    );

    assert_eq!(out, "A-ctor|B-head:tail|C-ctor");
}

/// Verifies AOT constructor by-reference variadic args write back caller variables.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_variadic_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorVariadicRefBridge {
    public string $label;

    public function __construct(&...$items) {
        $items[0] = $items[0] . "-ctor";
        $items[1] = $items[1] . "-tail";
        $this->label = $items[0] . ":" . $items[1];
    }
}

echo eval('$a = "A";
$b = "B";
$box = new EvalCtorVariadicRefBridge($a, $b);
echo $box->label . "|";
return $a . ":" . $b;');
"#,
    );

    assert_eq!(out, "A-ctor:B-tail|A-ctor:B-tail");
}

/// Verifies AOT constructor by-reference writeback happens before a catchable throw.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_lvalue_writeback_before_throw() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorThrowRefTargetBridge {
    public function __construct(int &$value) {
        $value = $value + 11;
        throw new Exception("ctor-lvalue");
    }
}

class EvalCtorThrowRefTargetBox {
    public int $value = 5;
}

echo eval('$items = ["x" => "1"];
try {
    new EvalCtorThrowRefTargetBridge($items["x"]);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$box = new EvalCtorThrowRefTargetBox();
try {
    new EvalCtorThrowRefTargetBridge($box->value);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
return gettype($box->value) . ":" . $box->value;');
"#,
    );

    assert_eq!(
        out,
        "Exception:ctor-lvalue:integer:12|Exception:ctor-lvalue:integer:16"
    );
}

/// Verifies AOT constructor argument-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_arg_prep_fatal_cleans_up_stack() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCtorPrepFatalNeed {}
class EvalCtorPrepFatalBridge {
    public function __construct(int &$value, EvalCtorPrepFatalNeed $need) {
        $value = $value + 1;
    }
}

echo eval('$value = "2";
new EvalCtorPrepFatalBridge($value, 123);
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

/// Verifies named and unpacked AOT constructor arg-prep fatals restore the eval bridge frame.
#[test]
fn test_eval_dynamic_new_constructor_by_ref_named_spread_arg_prep_fatal_cleans_up_stack() {
    let cases = [
        (
            "named",
            r#"<?php
class EvalCtorNamedPrepFatalNeed {}
class EvalCtorNamedPrepFatalBridge {
    public function __construct(int &$value, EvalCtorNamedPrepFatalNeed $need) {
        $value = $value + 1;
    }
}

echo eval('$class = "EvalCtorNamedPrepFatalBridge";
$value = "2";
new $class(value: $value, need: 123);
echo "bad";');
"#,
        ),
        (
            "spread",
            r#"<?php
class EvalCtorSpreadPrepFatalNeed {}
class EvalCtorSpreadPrepFatalBridge {
    public function __construct(int &$value, EvalCtorSpreadPrepFatalNeed $need) {
        $value = $value + 1;
    }
}

echo eval('$class = "EvalCtorSpreadPrepFatalBridge";
$value = "2";
new $class(...["value" => &$value, "need" => 123]);
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

/// Verifies eval-declared constructor by-reference args write back to lvalue targets.
#[test]
fn test_eval_declared_constructor_by_ref_writes_back_to_lvalue_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalDeclaredCtorRefTargetBridge {
    public function __construct(int &$value) {
        $value = $value + 5;
    }
}

class EvalDeclaredCtorRefTargetBox {
    public int $value = 3;
}

class EvalDeclaredCtorRefTargetStatic {
    public static mixed $value = 4;
}

$value = "1";
new EvalDeclaredCtorRefTargetBridge($value);
echo gettype($value) . ":" . $value . "|";

$items = ["x" => "2"];
new EvalDeclaredCtorRefTargetBridge($items["x"]);
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$nested = ["outer" => ["inner" => "3"]];
new EvalDeclaredCtorRefTargetBridge($nested["outer"]["inner"]);
echo gettype($nested["outer"]["inner"]) . ":" . $nested["outer"]["inner"] . "|";

$box = new EvalDeclaredCtorRefTargetBox();
new EvalDeclaredCtorRefTargetBridge($box->value);
echo gettype($box->value) . ":" . $box->value . "|";

EvalDeclaredCtorRefTargetStatic::$value = "5";
new EvalDeclaredCtorRefTargetBridge(EvalDeclaredCtorRefTargetStatic::$value);
return gettype(EvalDeclaredCtorRefTargetStatic::$value) . ":" . EvalDeclaredCtorRefTargetStatic::$value;');
"#,
    );

    assert_eq!(
        out,
        "integer:6|integer:7|integer:8|integer:8|integer:10"
    );
}

/// Verifies eval-declared constructor by-reference writeback happens before catchable throw.
#[test]
fn test_eval_declared_constructor_by_ref_lvalue_writeback_before_throw() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalDeclaredCtorThrowRefTargetBridge {
    public function __construct(int &$value) {
        $value = $value + 11;
        throw new Exception("eval-ctor-lvalue");
    }
}

class EvalDeclaredCtorThrowRefTargetBox {
    public int $value = 5;
}

$items = ["x" => "1"];
try {
    new EvalDeclaredCtorThrowRefTargetBridge($items["x"]);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
echo gettype($items["x"]) . ":" . $items["x"] . "|";

$box = new EvalDeclaredCtorThrowRefTargetBox();
try {
    new EvalDeclaredCtorThrowRefTargetBridge($box->value);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
return gettype($box->value) . ":" . $box->value;');
"#,
    );

    assert_eq!(
        out,
        "Exception:eval-ctor-lvalue:integer:12|Exception:eval-ctor-lvalue:integer:16"
    );
}
