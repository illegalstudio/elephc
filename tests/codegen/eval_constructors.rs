//! Purpose:
//! End-to-end regressions for runtime eval object construction through AOT classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_constructor` through Rust's test harness.
//!
//! Key details:
//! - Fixtures focus on constructor bridge argument binding and by-reference
//!   writeback for non-variable eval caller targets.
//! - A constructor bridge is called directly, one argument per PHYSICAL parameter, so the
//!   argument-frame fixture also covers the compiler-internal count and collector slots eval has
//!   to materialize on top of the PHP-visible signature.

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

$argsCopy = "5";
$aotRef->newInstanceArgs([$argsCopy]);
echo gettype($argsCopy) . ":" . $argsCopy . "|";

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
echo gettype($evalArgsValue) . ":" . $evalArgsValue . "|";

$evalArgsCopy = "6";
$evalRef->newInstanceArgs([$evalArgsCopy]);
return gettype($evalArgsCopy) . ":" . $evalArgsCopy;');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "string:1|integer:7|string:5|string:3|integer:11|string:6"
    );
    for warning in [
        "EvalReflectAotCtorRefBridge::__construct(): Argument #1 ($value) must be passed by reference, value given",
        "EvalReflectDeclaredCtorRefBridge::__construct(): Argument #1 ($value) must be passed by reference, value given",
    ] {
        let count = out.stderr.matches(warning).count();
        assert!(
            count >= 2,
            "expected at least two by-ref warnings {warning:?}, saw {count}: {}",
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

/// An eval `new` into an AOT constructor reports the frame the PHP source declares.
///
/// A constructor bridge is reached the same way a method bridge is: eval supplies one argument
/// per PHYSICAL parameter, including the compiler-internal argument count and, when the source
/// declares no variadic of its own, the hidden surplus collector whose first element carries
/// that count. The two shapes are covered separately because they materialize different hidden
/// slots, and each is exercised with the optional both omitted and supplied plus a surplus tail.
#[test]
fn test_eval_aot_constructor_frames_follow_the_declared_signature() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorVariadicFrameShape {
    public string $tally = "";

    public function __construct($first, $second = 5, ...$rest) {
        $this->tally = func_num_args() . ":" . implode(",", func_get_args())
            . ":" . implode(",", $rest);
    }
}

class EvalCtorCollectorFrameShape {
    public string $tally = "";

    public function __construct($first, $second = 5) {
        $this->tally = func_num_args() . ":" . implode(",", func_get_args());
    }
}

echo eval('$a = new EvalCtorVariadicFrameShape(1);
echo $a->tally, "|";
$b = new EvalCtorVariadicFrameShape(1, 2, 3, 4);
echo $b->tally, "|";
$c = new EvalCtorCollectorFrameShape(1);
echo $c->tally, "|";
$d = new EvalCtorCollectorFrameShape(1, 2, 3);
echo $d->tally, "|";
$e = new EvalCtorCollectorFrameShape(first: 1);
return $e->tally;');
"#,
    );

    assert_eq!(out, "1:1:|4:1,2,3,4:3,4|1:1|3:1,2,3|1:1");
}

/// An eval `new` into an AOT constructor whose optional default has NO eval representation still
/// sees the optional as optional, and still reports the real argument count.
///
/// Same contract as the method fixture in `tests/codegen/eval_callables.rs`, on the constructor
/// registration path, which registers its own shape through its own ABI entry point. The default
/// nests twenty array levels deep, past `MAX_NATIVE_DEFAULT_CONSTANT_DEPTH`, so no default is
/// registered for `$second`; an enum-case default is unrepresentable for the same reason. Without
/// the explicit shape, `new EvalCtorUnrepresentableDefault(1)` would be rejected as missing a
/// mandatory argument, and the hidden collector would carry no count for `func_num_args()`.
///
/// The named-argument call at the end also pins that no hidden slot became reachable by name in
/// the process: hidden slots register the empty name, and a PHP parameter name is never empty.
#[test]
fn test_eval_aot_constructor_optional_default_without_an_eval_representation_stays_optional() {
    let out = compile_and_run(
        r#"<?php
class EvalCtorUnrepresentableDefault {
    public string $tally = "";

    public function __construct($first, $second = [[[[[[[[[[[[[[[[[[[[1]]]]]]]]]]]]]]]]]]]]) {
        $this->tally = func_num_args() . ":" . count(func_get_args()) . ":" . count($second);
    }
}

echo eval('$a = new EvalCtorUnrepresentableDefault(1);
echo $a->tally, "|";
$b = new EvalCtorUnrepresentableDefault(1, [7, 8, 9]);
echo $b->tally, "|";
$c = new EvalCtorUnrepresentableDefault(first: 1);
return $c->tally;');
"#,
    );

    assert_eq!(out, "1:1:1|2:2:3|1:1:1");
}
