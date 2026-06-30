//! Purpose:
//! End-to-end regressions for eval callable bridge dispatch.
//! Covers callable values crossing eval into generated/AOT functions and methods.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_callable` through Rust's test harness.
//!
//! Key details:
//! - Fixtures verify by-reference writeback through string, callable-array, and
//!   first-class callable forms instead of only direct method/function syntax.

use crate::support::compile_and_run;

/// Verifies eval string and first-class AOT function callables preserve by-ref writeback.
#[test]
fn test_eval_aot_function_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
function eval_aot_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

echo eval('$string = "eval_aot_ref_add";
$a = "2";
echo $string($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$first = eval_aot_ref_add(...);
$b = "4";
echo $first($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$c = "6";
return call_user_func_array($first, [&$c, 7]) . ":" . gettype($c) . ":" . $c;');
"#,
    );

    assert_eq!(out, "5:integer:5|9:integer:9|13:integer:13");
}

/// Verifies eval callable-array AOT methods preserve by-ref writeback.
#[test]
fn test_eval_aot_callable_arrays_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalAotCallableArrayRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotCallableArrayRefBox();
$instance = [$box, "bump"];
$a = "2";
echo $instance($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$b = "4";
echo call_user_func_array($instance, [&$b, 5]) . ":" . gettype($b) . ":" . $b . "|";
$static = ["EvalAotCallableArrayRefBox", "add"];
$c = "7";
echo $static($c, 6) . ":" . gettype($c) . ":" . $c . "|";
$d = "8";
return call_user_func_array($static, [&$d, 9]) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(
        out,
        "15:integer:15|19:integer:19|13:integer:13|17:integer:17"
    );
}

/// Verifies eval first-class AOT method callables preserve by-ref writeback.
#[test]
fn test_eval_aot_first_class_method_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalAotFirstClassRefBox {
    public int $base = 20;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotFirstClassRefBox();
$method = $box->bump(...);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$static = EvalAotFirstClassRefBox::add(...);
$b = "4";
echo $static($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$name = "EvalAotFirstClassRefBox::add";
$c = "6";
echo $name($c, 7) . ":" . gettype($c) . ":" . $c . "|";
$d = "8";
return call_user_func_array($static, [&$d, 9]) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(
        out,
        "25:integer:25|9:integer:9|13:integer:13|17:integer:17"
    );
}

/// Verifies eval first-class callables are PHP-visible `Closure` objects and remain invokable.
#[test]
fn test_eval_first_class_callables_are_php_closure_objects() {
    let out = compile_and_run(
        r#"<?php
function eval_aot_first_class_object_func(string $value): string {
    return "F" . $value;
}

class EvalAotFirstClassObjectBox {
    public function m(string $value): string {
        return "M" . $value;
    }

    public static function s(string $value): string {
        return "S" . $value;
    }

    public function __invoke(string $value): string {
        return "I" . $value;
    }
}

echo eval('$box = new EvalAotFirstClassObjectBox();
$f = eval_aot_first_class_object_func(...);
$m = $box->m(...);
$s = EvalAotFirstClassObjectBox::s(...);
$class = "EvalAotFirstClassObjectBox";
$ds = $class::s(...);
$i = $box(...);
foreach ([$f, $m, $s, $ds, $i] as $cb) {
    echo is_object($cb) ? "O" : "o";
    echo get_class($cb);
    echo $cb instanceof Closure ? "I" : "i";
    echo is_callable($cb) ? "C" : "c";
    echo "|";
}
return $f("1") . ":" . $m("2") . ":" . $s("3") . ":" . $ds("4") . ":" . $i("5");');
"#,
    );

    assert_eq!(
        out,
        "OClosureIC|OClosureIC|OClosureIC|OClosureIC|OClosureIC|F1:M2:S3:S4:I5"
    );
}

/// Verifies `Closure::fromCallable()` normalizes eval string and array callables to Closure objects.
#[test]
fn test_eval_closure_from_callable_normalizes_string_and_array_callables() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_fn(int $value): string {
    return "F" . $value;
}

class EvalFromCallableBox {
    public function m(int $value): string {
        return "M" . $value;
    }

    public static function s(int $value): string {
        return "S" . $value;
    }
}

echo eval('$box = new EvalFromCallableBox();
$callbacks = [
    Closure::fromCallable("eval_from_callable_fn"),
    Closure::fromCallable([$box, "m"]),
    Closure::fromCallable(["EvalFromCallableBox", "s"]),
    Closure::fromCallable("EvalFromCallableBox::s"),
];
foreach ($callbacks as $index => $cb) {
    echo is_object($cb) ? "O" : "o";
    echo get_class($cb);
    echo $cb instanceof Closure ? "I" : "i";
    echo is_callable($cb) ? "C" : "c";
    echo $cb($index + 1);
    echo "|";
}');
"#,
    );

    assert_eq!(
        out,
        "OClosureICF1|OClosureICM2|OClosureICS3|OClosureICS4|"
    );
}

/// Verifies eval string callback APIs reject missing functions with PHP TypeErrors.
#[test]
fn test_eval_callable_validation_missing_string_callables_raise_type_errors() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    call_user_func("MiSsInG_Eval_Callback");
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array("OtherMissingEvalCallback", []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable("ThirdMissingEvalCallback");
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );

    assert_eq!(
        out,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, function \"MiSsInG_Eval_Callback\" not found or invalid function name|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, function \"OtherMissingEvalCallback\" not found or invalid function name|\
TypeError:Failed to create closure from callable: function \"ThirdMissingEvalCallback\" not found or invalid function name"
    );
}

/// Verifies `Closure::fromCallable()` rejects invalid object and method targets.
#[test]
fn test_eval_callable_validation_closure_from_callable_rejects_invalid_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFromCallablePlain {}
class EvalFromCallableMissing {}
class EvalFromCallablePrivate {
    private function hidden() {
        return "bad";
    }
}
class EvalFromCallableInstance {
    public function inst() {
        return "bad";
    }
}

try {
    Closure::fromCallable(new EvalFromCallablePlain());
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable([new EvalFromCallableMissing(), "MiSsInG"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable([new EvalFromCallablePrivate(), "hidden"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    Closure::fromCallable(["EvalFromCallableInstance", "inst"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );

    assert_eq!(
        out,
        "TypeError:Failed to create closure from callable: no array or string given|\
TypeError:Failed to create closure from callable: class EvalFromCallableMissing does not have a method \"MiSsInG\"|\
TypeError:Failed to create closure from callable: cannot access private method EvalFromCallablePrivate::hidden()|\
TypeError:Failed to create closure from callable: non-static method EvalFromCallableInstance::inst() cannot be called statically"
    );
}

/// Verifies `Closure::fromCallable()` preserves by-ref writeback for AOT call targets.
#[test]
fn test_eval_closure_from_callable_preserves_aot_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalFromCallableRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalFromCallableRefBox();
$function = Closure::fromCallable("eval_from_callable_ref_add");
$a = "2";
echo $function($a, 3) . ":" . gettype($a) . ":" . $a . "|";
$instance = Closure::fromCallable([$box, "bump"]);
$b = "4";
echo $instance($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$static = Closure::fromCallable(["EvalFromCallableRefBox", "add"]);
$c = "7";
echo $static($c, 6) . ":" . gettype($c) . ":" . $c . "|";
$named = Closure::fromCallable("EvalFromCallableRefBox::add");
$d = "8";
return call_user_func_array($named, [&$d, 9]) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(
        out,
        "5:integer:5|19:integer:19|13:integer:13|17:integer:17"
    );
}

/// Verifies `Closure::call()` rebinds `fromCallable()` method closures to a same-class receiver.
#[test]
fn test_eval_closure_from_callable_call_rebinds_same_class_method_and_invokable_targets() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_call_fn(): string {
    return "bad";
}

class EvalFromCallableCallBox {
    public int $base = 0;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int $value): int {
        return $value + 1;
    }
}

echo eval('$original = new EvalFromCallableCallBox();
$original->base = 10;
$bound = new EvalFromCallableCallBox();
$bound->base = 20;

$method = Closure::fromCallable([$original, "bump"]);
$a = "2";
echo $method->call($bound, $a, 3) . ":" . gettype($a) . ":" . $a . "|";

$invoke = Closure::fromCallable($original);
$b = "4";
echo $invoke->call($bound, $b, 5) . ":" . gettype($b) . ":" . $b . "|";

$function = Closure::fromCallable("eval_from_callable_call_fn");
echo is_null($function->call($bound)) ? "F" : "f"; echo "|";

$static = Closure::fromCallable(["EvalFromCallableCallBox", "add"]);
echo is_null($static->call($bound, 1)) ? "S" : "s";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|F|S");
}

/// Verifies `Closure::bindTo()` persists rebinding for method and invokable callable targets.
#[test]
fn test_eval_closure_bind_from_callable_persists_method_and_invokable_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalFromCallableBindBox {
    public int $base = 0;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int $value): int {
        return $value + 1;
    }
}

echo eval('$original = new EvalFromCallableBindBox();
$original->base = 10;
$bound = new EvalFromCallableBindBox();
$bound->base = 20;

$rawMethod = Closure::fromCallable([$original, "bump"]);
$method = $rawMethod->bindTo($bound);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$rawInvoke = Closure::fromCallable($original);
$invoke = $rawInvoke->bindTo($bound);
$b = "4";
echo call_user_func_array($invoke, [&$b, 5]) . ":" . gettype($b) . ":" . $b . "|";

$static = Closure::fromCallable(["EvalFromCallableBindBox", "add"]);
echo is_null($static->bindTo($bound)) ? "S" : "s";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|S");
}
