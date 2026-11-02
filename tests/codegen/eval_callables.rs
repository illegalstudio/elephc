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
//! - The argument-frame fixtures pin the two different eval call ABIs apart: a free function is
//!   reached through a descriptor invoker that synthesizes the hidden `func_args` slots from a
//!   container of only the supplied arguments, while a method bridge is called directly with one
//!   argument per physical parameter, hidden slots included.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
    compile_and_run_with_regex,
};

/// Interpreted object-method callable arrays release the receiver their array read retained.
///
/// Normalizing `[$t, 'm']` reads the receiver out of the array with one owned reference, which
/// the dispatch then dropped: `$cb(7)`, `call_user_func()` (positional and named), and
/// `call_user_func_array()` each leaked one 48-byte box per call while `$t->m(7)` stayed clean.
#[test]
fn test_eval_object_callable_arrays_release_their_receiver_after_the_call() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
eval('
class LeakProbe { public function m($v) { return $v * 2; } }
$t = new LeakProbe();
$sum = 0;
for ($i = 0; $i < 40; $i++) {
    $cb = [$t, "m"];
    $sum += $cb(1);
    $sum += call_user_func([$t, "m"], 1);
    $sum += call_user_func([$t, "m"], v: 1);
    $sum += call_user_func_array([$t, "m"], [1]);
}
echo $sum, "\n";
');
"#,
    );
    assert_eq!(out.stdout, "320\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "callable-array receivers leaked:\n{}",
        out.stderr
    );
}

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

/// Verifies eval AOT callables preserve named by-ref argument writeback.
#[test]
fn test_eval_aot_callable_named_ref_args_preserve_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalAotCallableNamedRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotCallableNamedRefBox();

$array = [$box, "bump"];
$a = "2";
echo $array(value: $a, delta: 3) . ":" . gettype($a) . ":" . $a . "|";

$first = $box->bump(...);
$b = "4";
echo $first(delta: 5, value: $b) . ":" . gettype($b) . ":" . $b . "|";

$closure = Closure::fromCallable([$box, "bump"]);
$c = "6";
echo $closure(delta: 7, value: $c) . ":" . gettype($c) . ":" . $c . "|";

$string = "EvalAotCallableNamedRefBox::add";
$d = "8";
echo $string(delta: 9, value: $d) . ":" . gettype($d) . ":" . $d . "|";

$static = EvalAotCallableNamedRefBox::add(...);
$e = "10";
echo $static(value: $e, delta: 11) . ":" . gettype($e) . ":" . $e . "|";

$invokable = new EvalAotCallableNamedRefBox();
$f = "12";
return $invokable(delta: 13, value: $f) . ":" . gettype($f) . ":" . $f;');
"#,
    );

    assert_eq!(
        out,
        "15:integer:15|19:integer:19|23:integer:23|17:integer:17|21:integer:21|35:integer:35"
    );
}

/// Verifies eval `call_user_func_array()` preserves named AOT by-ref argument aliases.
#[test]
fn test_eval_call_user_func_array_aot_callable_named_ref_args_preserve_writeback() {
    let out = compile_and_run(
        r#"<?php
function eval_call_array_aot_named_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalCallArrayAotNamedRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

echo eval('$function = "eval_call_array_aot_named_ref_add";
$a = "2";
echo call_user_func_array($function, ["delta" => 3, "value" => &$a]) .
    ":" . gettype($a) . ":" . $a . "|";

$first = eval_call_array_aot_named_ref_add(...);
$b = "4";
echo call_user_func_array($first, ["delta" => 5, "value" => &$b]) .
    ":" . gettype($b) . ":" . $b . "|";

$box = new EvalCallArrayAotNamedRefBox();
$array = [$box, "bump"];
$c = "6";
echo call_user_func_array($array, ["delta" => 7, "value" => &$c]) .
    ":" . gettype($c) . ":" . $c . "|";

$closure = Closure::fromCallable([$box, "bump"]);
$d = "8";
echo call_user_func_array($closure, ["value" => &$d, "delta" => 9]) .
    ":" . gettype($d) . ":" . $d . "|";

$string = "EvalCallArrayAotNamedRefBox::add";
$e = "10";
echo call_user_func_array($string, ["delta" => 11, "value" => &$e]) .
    ":" . gettype($e) . ":" . $e . "|";

$static = EvalCallArrayAotNamedRefBox::add(...);
$f = "12";
echo call_user_func_array($static, ["value" => &$f, "delta" => 13]) .
    ":" . gettype($f) . ":" . $f . "|";

$invokable = new EvalCallArrayAotNamedRefBox();
$g = "14";
return call_user_func_array($invokable, ["delta" => 15, "value" => &$g]) .
    ":" . gettype($g) . ":" . $g;');
"#,
    );

    assert_eq!(
        out,
        "5:integer:5|9:integer:9|23:integer:23|27:integer:27|21:integer:21|25:integer:25|39:integer:39"
    );
}

/// Verifies eval `call_user_func_array()` preserves named eval-declared by-ref aliases.
#[test]
fn test_eval_call_user_func_array_declared_callable_named_ref_args_preserve_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('function eval_call_array_declared_named_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalCallArrayDeclaredNamedRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

$function = "eval_call_array_declared_named_ref_add";
$a = "2";
echo call_user_func_array($function, ["delta" => 3, "value" => &$a]) .
    ":" . gettype($a) . ":" . $a . "|";

$first = eval_call_array_declared_named_ref_add(...);
$b = "4";
echo call_user_func_array($first, ["delta" => 5, "value" => &$b]) .
    ":" . gettype($b) . ":" . $b . "|";

$box = new EvalCallArrayDeclaredNamedRefBox();
$array = [$box, "bump"];
$c = "6";
echo call_user_func_array($array, ["delta" => 7, "value" => &$c]) .
    ":" . gettype($c) . ":" . $c . "|";

$closure = Closure::fromCallable([$box, "bump"]);
$d = "8";
echo call_user_func_array($closure, ["value" => &$d, "delta" => 9]) .
    ":" . gettype($d) . ":" . $d . "|";

$string = "EvalCallArrayDeclaredNamedRefBox::add";
$e = "10";
echo call_user_func_array($string, ["delta" => 11, "value" => &$e]) .
    ":" . gettype($e) . ":" . $e . "|";

$static = EvalCallArrayDeclaredNamedRefBox::add(...);
$f = "12";
echo call_user_func_array($static, ["value" => &$f, "delta" => 13]) .
    ":" . gettype($f) . ":" . $f . "|";

$invokable = new EvalCallArrayDeclaredNamedRefBox();
$g = "14";
return call_user_func_array($invokable, ["delta" => 15, "value" => &$g]) .
    ":" . gettype($g) . ":" . $g;');
"#,
    );

    assert_eq!(
        out,
        "5:integer:5|9:integer:9|23:integer:23|27:integer:27|21:integer:21|25:integer:25|39:integer:39"
    );
}

/// Verifies eval `call_user_func()` keeps AOT callable by-reference args by value.
#[test]
fn test_eval_call_user_func_aot_callable_forms_use_by_value_args() {
    let out = compile_and_run(
        r#"<?php
function eval_call_user_func_aot_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalCallUserFuncAotRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

echo eval('$string = "eval_call_user_func_aot_ref_add";
$a = "2";
echo call_user_func($string, $a, 3) . ":" . gettype($a) . ":" . $a . "|";

$first = eval_call_user_func_aot_ref_add(...);
$b = "4";
echo call_user_func($first, $b, 5) . ":" . gettype($b) . ":" . $b . "|";

$box = new EvalCallUserFuncAotRefBox();
$array = [$box, "bump"];
$c = "6";
echo call_user_func($array, $c, 7) . ":" . gettype($c) . ":" . $c . "|";

$staticArray = ["EvalCallUserFuncAotRefBox", "add"];
$d = "8";
echo call_user_func($staticArray, $d, 9) . ":" . gettype($d) . ":" . $d . "|";

$staticString = "EvalCallUserFuncAotRefBox::add";
$e = "10";
echo call_user_func($staticString, $e, 11) . ":" . gettype($e) . ":" . $e . "|";

$invokable = new EvalCallUserFuncAotRefBox();
$f = "12";
return call_user_func($invokable, $f, 13) . ":" . gettype($f) . ":" . $f;');
"#,
    );

    assert_eq!(
        out,
        "5:string:2|9:string:4|23:string:6|17:string:8|21:string:10|35:string:12"
    );
}

/// Verifies eval-declared callable forms preserve by-ref writeback.
#[test]
fn test_eval_declared_callable_forms_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('function eval_declared_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalDeclaredRefCallableBox {
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

$string = "eval_declared_ref_add";
$a = "2";
echo $string($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$first = eval_declared_ref_add(...);
$b = "4";
echo $first($b, 5) . ":" . gettype($b) . ":" . $b . "|";
$c = "6";
echo call_user_func_array($first, [&$c, 7]) . ":" . gettype($c) . ":" . $c . "|";

$box = new EvalDeclaredRefCallableBox();
$instance = [$box, "bump"];
$d = "8";
echo $instance($d, 4) . ":" . gettype($d) . ":" . $d . "|";
$e = "1";
echo call_user_func_array($instance, [&$e, 5]) . ":" . gettype($e) . ":" . $e . "|";

$static = ["EvalDeclaredRefCallableBox", "add"];
$f = "7";
echo $static($f, 6) . ":" . gettype($f) . ":" . $f . "|";

$closureFunction = Closure::fromCallable("eval_declared_ref_add");
$g = "3";
echo $closureFunction($g, 4) . ":" . gettype($g) . ":" . $g . "|";

$closureInstance = Closure::fromCallable([$box, "bump"]);
$h = "2";
echo $closureInstance($h, 6) . ":" . gettype($h) . ":" . $h . "|";

$closureStatic = Closure::fromCallable(["EvalDeclaredRefCallableBox", "add"]);
$i = "5";
echo $closureStatic($i, 8) . ":" . gettype($i) . ":" . $i . "|";

$closureNamedStatic = Closure::fromCallable("EvalDeclaredRefCallableBox::add");
$j = "6";
return call_user_func_array($closureNamedStatic, [&$j, 9]) . ":" . gettype($j) . ":" . $j;');
"#,
    );

    assert_eq!(
        out,
        concat!(
            "5:integer:5|9:integer:9|13:integer:13|22:integer:22|16:integer:16|",
            "13:integer:13|7:integer:7|18:integer:18|13:integer:13|15:integer:15"
        )
    );
}

/// Verifies eval-declared callable forms preserve by-ref variadic element writeback.
#[test]
fn test_eval_declared_callable_forms_preserve_by_ref_variadic_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalDeclaredVariadicRefCallableBox {
    public function collect(&...$items): string {
        $items[0] = $items[0] . "-i";
        $items["named"] = $items["named"] . "-n";
        return $items[0] . ":" . $items["named"];
    }

    public static function collectStatic(&...$items): string {
        $items[0] = $items[0] . "-s";
        $items["named"] = $items["named"] . "-sn";
        return $items[0] . ":" . $items["named"];
    }
}

$box = new EvalDeclaredVariadicRefCallableBox();

$array = [$box, "collect"];
$a = "A";
$b = "B";
echo $array($a, named: $b) . ":" . $a . ":" . $b . "|";

$first = $box->collect(...);
$c = "C";
$d = "D";
echo $first($c, named: $d) . ":" . $c . ":" . $d . "|";

$closure = Closure::fromCallable([$box, "collect"]);
$e = "E";
$f = "F";
echo $closure($e, named: $f) . ":" . $e . ":" . $f . "|";

$string = "EvalDeclaredVariadicRefCallableBox::collectStatic";
$g = "G";
$h = "H";
echo $string($g, named: $h) . ":" . $g . ":" . $h . "|";

$i = "I";
$j = "J";
$args = [&$i, "named" => &$j];
return call_user_func_array($closure, $args) . ":" . $i . ":" . $j . ":" .
    $args[0] . ":" . $args["named"];');
"#,
    );

    assert_eq!(
        out,
        concat!(
            "A-i:B-n:A-i:B-n|C-i:D-n:C-i:D-n|E-i:F-n:E-i:F-n|",
            "G-s:H-sn:G-s:H-sn|I-i:J-n:I-i:J-n:I-i:J-n"
        )
    );
}

/// Verifies AOT function callable forms preserve by-ref variadic element writeback.
#[test]
fn test_eval_aot_function_callable_forms_preserve_by_ref_variadic_writeback() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_variadic_ref_collect(&...$items): string {
    $items[0] = $items[0] . "-f";
    $items[1] = $items[1] . "-g";
    return $items[0] . ":" . $items[1];
}

echo eval('$string = "eval_aot_variadic_ref_collect";
$a = "A";
$b = "B";
echo $string($a, $b) . ":" . $a . ":" . $b . "|";

$first = eval_aot_variadic_ref_collect(...);
$c = "C";
$d = "D";
echo $first($c, $d) . ":" . $c . ":" . $d . "|";

$closure = Closure::fromCallable("eval_aot_variadic_ref_collect");
$e = "E";
$f = "F";
echo $closure($e, $f) . ":" . $e . ":" . $f . "|";

$g = "G";
$h = "H";
$args = [&$g, &$h];
return call_user_func_array($closure, $args) . ":" . $g . ":" . $h . ":" .
    $args[0] . ":" . $args[1];');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "A-f:B-g:A-f:B-g|C-f:D-g:C-f:D-g|E-f:F-g:E-f:F-g|",
            "G-f:H-g:G-f:H-g:G-f:H-g"
        )
    );
}

/// Verifies AOT method callable forms preserve by-ref variadic element writeback.
#[test]
fn test_eval_aot_method_callable_forms_preserve_by_ref_variadic_writeback() {
    let out = compile_and_run(
        r#"<?php
class EvalAotVariadicRefCallableBox {
    public function collect(&...$items): string {
        $items[0] = $items[0] . "-i";
        $items[1] = $items[1] . "-j";
        return $items[0] . ":" . $items[1];
    }

    public static function collectStatic(&...$items): string {
        $items[0] = $items[0] . "-s";
        $items[1] = $items[1] . "-t";
        return $items[0] . ":" . $items[1];
    }
}

echo eval('$box = new EvalAotVariadicRefCallableBox();

$array = [$box, "collect"];
$a = "A";
$b = "B";
echo $array($a, $b) . ":" . $a . ":" . $b . "|";

$first = $box->collect(...);
$c = "C";
$d = "D";
echo $first($c, $d) . ":" . $c . ":" . $d . "|";

$closure = Closure::fromCallable([$box, "collect"]);
$e = "E";
$f = "F";
echo $closure($e, $f) . ":" . $e . ":" . $f . "|";

$string = "EvalAotVariadicRefCallableBox::collectStatic";
$g = "G";
$h = "H";
echo $string($g, $h) . ":" . $g . ":" . $h . "|";

$static = EvalAotVariadicRefCallableBox::collectStatic(...);
$i = "I";
$j = "J";
echo $static($i, $j) . ":" . $i . ":" . $j . "|";

$k = "K";
$l = "L";
$args = [&$k, &$l];
return call_user_func_array($closure, $args) . ":" . $k . ":" . $l . ":" .
    $args[0] . ":" . $args[1];');
"#,
    );

    assert_eq!(
        out,
        concat!(
            "A-i:B-j:A-i:B-j|C-i:D-j:C-i:D-j|E-i:F-j:E-i:F-j|",
            "G-s:H-t:G-s:H-t|I-s:J-t:I-s:J-t|K-i:L-j:K-i:L-j:K-i:L-j"
        )
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

/// Verifies namespaced eval first-class function callables use PHP builtin fallback rules.
#[test]
fn test_eval_first_class_function_callables_follow_namespace_fallback() {
    let out = compile_and_run(
        r#"<?php
namespace EvalFirstClassFallback;

eval('namespace EvalFirstClassFallback;
function strrev($value) {
    return "local:" . $value;
}

$builtin = strlen(...);
$local = strrev(...);
echo $builtin("abcd") . "|";
echo $local("ab") . "|";
echo call_user_func($builtin, "xyz") . "|";
echo call_user_func_array($local, ["cd"]);');
"#,
    );

    assert_eq!(out, "4|local:ab|3|local:cd");
}

/// Verifies eval first-class function callables resolve namespace function imports.
#[test]
fn test_eval_first_class_function_callables_follow_namespace_imports() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalFirstClassImport\Lib;
function target($value) {
    return "target:" . $value;
}

namespace EvalFirstClassImport\App;
use function strlen as Len;
use function EvalFirstClassImport\Lib\target as AliasTarget;

$len = Len(...);
$target = AliasTarget(...);
echo $len("abcd") . "|";
echo $target("x") . "|";
echo call_user_func($len, "yz") . "|";
echo call_user_func_array($target, ["q"]);');
"#,
    );

    assert_eq!(out, "4|target:x|2|target:q");
}

/// Verifies callable-array class receivers accept PHP's leading namespace separator.
#[test]
fn test_eval_callable_array_class_receivers_allow_leading_namespace_separator() {
    let out = compile_and_run(
        r#"<?php
class EvalLeadingSlashAotCallableArray {
    public static function stat(string $value): string {
        return "A" . $value;
    }
}

echo eval('class EvalLeadingSlashCallableArray {
    public static function stat($value) {
        return "E" . $value;
    }
}

$eval = ["\\EvalLeadingSlashCallableArray", "stat"];
$aot = ["\\EvalLeadingSlashAotCallableArray", "stat"];
$name = "seed";

echo call_user_func($eval, "a") . "|";
echo $eval("b") . "|";
echo call_user_func_array($eval, ["c"]) . "|";
echo Closure::fromCallable($eval)("d") . "|";
echo is_callable($eval, false, $name) ? $name : "bad";
echo "|";
echo call_user_func($aot, "x") . "|";
echo $aot("y") . "|";
echo call_user_func_array($aot, ["z"]) . "|";
echo Closure::fromCallable($aot)("q");');
"#,
    );

    assert_eq!(
        out,
        "Ea|Eb|Ec|Ed|\\EvalLeadingSlashCallableArray::stat|Ax|Ay|Az|Aq"
    );
}

/// Verifies string callables accept PHP's leading namespace separator.
#[test]
fn test_eval_string_callables_allow_leading_namespace_separator() {
    let out = compile_and_run(
        r#"<?php
class EvalLeadingSlashAotStringCallable {
    public static function stat(string $value): string {
        return "A" . $value;
    }
}

echo eval('class EvalLeadingSlashStringCallable {
    public static function stat($value) {
        return "E" . $value;
    }
}

$builtin = "\\strlen";
$eval = "\\EvalLeadingSlashStringCallable::stat";
$aot = "\\EvalLeadingSlashAotStringCallable::stat";
$name = "seed";

echo $builtin("abcd") . "|";
echo call_user_func($eval, "a") . "|";
echo $eval("b") . "|";
echo call_user_func_array($eval, ["c"]) . "|";
echo Closure::fromCallable($eval)("d") . "|";
echo is_callable($eval, false, $name) ? $name : "bad";
echo "|";
echo call_user_func($aot, "x") . "|";
echo $aot("y") . "|";
echo call_user_func_array($aot, ["z"]) . "|";
echo Closure::fromCallable($aot)("q");');
"#,
    );

    assert_eq!(
        out,
        "4|Ea|Eb|Ec|Ed|\\EvalLeadingSlashStringCallable::stat|Ax|Ay|Az|Aq"
    );
}

/// Verifies eval first-class callables reject invalid method targets at creation time.
#[test]
fn test_eval_first_class_callable_validation_rejects_invalid_method_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassInvalidTargets {
    private function hidden() {
        return "bad";
    }

    public function inst() {
        return "bad";
    }

    private static function secret() {
        return "bad";
    }

    public static function ok() {
        return "OK";
    }
}

$box = new EvalFirstClassInvalidTargets();
try {
    $box->missing(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    $box->hidden(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalFirstClassInvalidTargets::missing(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalFirstClassInvalidTargets::inst(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalFirstClassInvalidTargets::secret(...);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
$ok = EvalFirstClassInvalidTargets::ok(...);
echo $ok();');
"#,
    );

    assert_eq!(
        out,
        "Error:Call to undefined method EvalFirstClassInvalidTargets::missing()|\
Error:Call to private method EvalFirstClassInvalidTargets::hidden() from global scope|\
Error:Call to undefined method EvalFirstClassInvalidTargets::missing()|\
Error:Non-static method EvalFirstClassInvalidTargets::inst() cannot be called statically|\
Error:Call to private method EvalFirstClassInvalidTargets::secret() from global scope|OK"
    );
}

/// Verifies eval first-class callables preserve PHP magic-method fallback.
#[test]
fn test_eval_first_class_callable_validation_preserves_magic_fallback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassMagicTargets {
    private function hidden() {
        return "bad";
    }

    private static function secret() {
        return "bad";
    }

    public function __call($name, $args) {
        return "I:" . $name . ":" . $args[0];
    }

    public static function __callStatic($name, $args) {
        return "S:" . $name . ":" . $args[0];
    }
}

$box = new EvalFirstClassMagicTargets();
$hidden = $box->hidden(...);
echo $hidden("a") . "|";
$missing = $box->missing(...);
echo $missing("b") . "|";
$secret = EvalFirstClassMagicTargets::secret(...);
echo $secret("c") . "|";
$staticMissing = EvalFirstClassMagicTargets::missingStatic(...);
echo $staticMissing("d");');
"#,
    );

    assert_eq!(out, "I:hidden:a|I:missing:b|S:secret:c|S:missingStatic:d");
}

/// Verifies `self::method(...)` inside an instance eval method captures `$this`.
#[test]
fn test_eval_first_class_callable_validation_static_syntax_instance_method_captures_this() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassStaticSyntaxThis {
    public int $base = 7;

    public function make() {
        $fn = self::add(...);
        return $fn(5);
    }

    public function add($value) {
        return $this->base + $value;
    }

    public static function makeStatic() {
        try {
            self::add(...);
            return "bad";
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }
    }
}

$box = new EvalFirstClassStaticSyntaxThis();
echo $box->make() . "|";
echo EvalFirstClassStaticSyntaxThis::makeStatic();');
"#,
    );

    assert_eq!(
        out,
        "12|Error:Non-static method EvalFirstClassStaticSyntaxThis::add() cannot be called statically"
    );
}

/// Verifies eval string callbacks resolve special class names through method scope.
#[test]
fn test_eval_string_special_class_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalStringSpecialCallableBase {
    public static function bump(int &$value, int $delta): int {
        $value += $delta;
        return $value;
    }
}

class EvalStringSpecialCallableChild extends EvalStringSpecialCallableBase {
    public int $base = 10;

    public function add(int &$value, int $delta): int {
        $value += $this->base + $delta;
        return $value;
    }

    public function run() {
        $self = "self::add";
        $first = "2";
        echo is_callable($self) ? "C:" : "c:";
        echo call_user_func_array($self, [&$first, 3]) . ":" . gettype($first) . ":" . $first . "|";

        $static = "static::add";
        $second = "4";
        echo call_user_func_array($static, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

        $parent = "parent::bump";
        $third = "6";
        echo call_user_func_array($parent, [&$third, 7]) . ":" . gettype($third) . ":" . $third;
    }
}

$box = new EvalStringSpecialCallableChild();
$box->run();');
"#,
    );

    assert_eq!(out, "C:15:integer:15|19:integer:19|13:integer:13");
}

/// Verifies eval first-class callbacks resolve special class names and preserve by-ref writeback.
#[test]
fn test_eval_first_class_special_class_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalFirstClassSpecialCallableBase {
    public static function bump(int &$value, int $delta): int {
        $value += $delta;
        return $value;
    }
}

class EvalFirstClassSpecialCallableChild extends EvalFirstClassSpecialCallableBase {
    public int $base = 10;

    public function add(int &$value, int $delta): int {
        $value += $this->base + $delta;
        return $value;
    }

    public function run() {
        $self = self::add(...);
        $first = "2";
        echo $self($first, 3) . ":" . gettype($first) . ":" . $first . "|";

        $static = static::add(...);
        $second = "4";
        echo call_user_func_array($static, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

        $parent = parent::bump(...);
        $third = "6";
        echo $parent($third, 7) . ":" . gettype($third) . ":" . $third;
    }
}

$box = new EvalFirstClassSpecialCallableChild();
$box->run();');
"#,
    );

    assert_eq!(out, "15:integer:15|19:integer:19|13:integer:13");
}

/// Verifies `Closure::fromCallable()` resolves special string callables through method scope.
#[test]
fn test_eval_closure_from_callable_special_string_callables_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFromCallableSpecialStringBase {
    public static function bump(int &$value, int $delta): int {
        $value += $delta;
        return $value;
    }
}

class EvalFromCallableSpecialStringChild extends EvalFromCallableSpecialStringBase {
    public int $base = 10;

    public function add(int &$value, int $delta): int {
        $value += $this->base + $delta;
        return $value;
    }

    public function run(): string {
        $self = Closure::fromCallable("self::add");
        $first = "2";
        $out = $self($first, 3) . ":" . gettype($first) . ":" . $first . "|";

        $static = Closure::fromCallable("static::add");
        $second = "4";
        $out .= call_user_func_array($static, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

        $parent = Closure::fromCallable("parent::bump");
        $third = "6";
        $out .= $parent($third, 7) . ":" . gettype($third) . ":" . $third;

        return $out;
    }
}

return (new EvalFromCallableSpecialStringChild())->run();');
"#,
    );

    assert_eq!(out, "15:integer:15|19:integer:19|13:integer:13");
}

/// Verifies special first-class instance closures retain `$this` after method scope exits.
#[test]
fn test_eval_first_class_special_instance_callables_persist_bound_receiver() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassPersistentInstanceBase {
    public function base(int &$value, int $delta): string {
        $value += $delta;
        return "B:" . get_called_class() . ":" . get_class($this) . ":" . $value;
    }
}

class EvalFirstClassPersistentInstanceChild extends EvalFirstClassPersistentInstanceBase {
    public int $base = 10;

    public function add(int &$value, int $delta): string {
        $value += $this->base + $delta;
        return "C:" . get_called_class() . ":" . get_class($this) . ":" . $value;
    }

    public function make(): array {
        return [
            self::add(...),
            static::add(...),
            parent::base(...),
        ];
    }
}

$closures = (new EvalFirstClassPersistentInstanceChild())->make();
$self = $closures[0];
$static = $closures[1];
$parent = $closures[2];

$first = "2";
$out = $self($first, 3) . ":" . gettype($first) . ":" . $first . "|";

$second = "3";
$out .= call_user_func_array($static, [&$second, 3]) . ":" . gettype($second) . ":" . $second . "|";

$third = "4";
$out .= $parent($third, 3) . ":" . gettype($third) . ":" . $third;

return $out;');
"#,
    );

    assert_eq!(
        out,
        "C:EvalFirstClassPersistentInstanceChild:EvalFirstClassPersistentInstanceChild:15:integer:15|\
C:EvalFirstClassPersistentInstanceChild:EvalFirstClassPersistentInstanceChild:16:integer:16|\
B:EvalFirstClassPersistentInstanceChild:EvalFirstClassPersistentInstanceChild:7:integer:7"
    );
}

/// Verifies special static string closures remain callable after leaving method scope.
#[test]
fn test_eval_closure_from_callable_special_static_string_callables_persist_resolved_scope() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFromCallablePersistentSpecialBase {
    public static function bump(int &$value, int $delta): string {
        $value += $delta;
        return "B:" . get_called_class() . ":" . $value;
    }
}

class EvalFromCallablePersistentSpecialChild extends EvalFromCallablePersistentSpecialBase {
    public static function add(int &$value, int $delta): string {
        $value += $delta;
        return "C:" . get_called_class() . ":" . $value;
    }

    public function make(): array {
        return [
            Closure::fromCallable("self::add"),
            Closure::fromCallable("static::add"),
            Closure::fromCallable("parent::bump"),
        ];
    }
}

$closures = (new EvalFromCallablePersistentSpecialChild())->make();
$self = $closures[0];
$static = $closures[1];
$parent = $closures[2];

$first = "2";
$out = $self($first, 3) . ":" . gettype($first) . ":" . $first . "|";

$second = "4";
$out .= call_user_func_array($static, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

$third = "6";
$out .= $parent($third, 7) . ":" . gettype($third) . ":" . $third;

return $out;');
"#,
    );

    assert_eq!(
        out,
        "C:EvalFromCallablePersistentSpecialChild:5:integer:5|\
C:EvalFromCallablePersistentSpecialChild:9:integer:9|\
B:EvalFromCallablePersistentSpecialChild:13:integer:13"
    );
}

/// Verifies special static array-callable closures remain callable after method scope exits.
#[test]
fn test_eval_closure_from_callable_special_static_array_callables_persist_resolved_scope() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFromCallablePersistentSpecialArrayBase {
    public static function bump(int &$value, int $delta): string {
        $value += $delta;
        return "B:" . get_called_class() . ":" . $value;
    }
}

class EvalFromCallablePersistentSpecialArrayChild extends EvalFromCallablePersistentSpecialArrayBase {
    public static function add(int &$value, int $delta): string {
        $value += $delta;
        return "C:" . get_called_class() . ":" . $value;
    }

    public static function make(): array {
        return [
            Closure::fromCallable(["self", "add"]),
            Closure::fromCallable(["static", "add"]),
            Closure::fromCallable(["parent", "bump"]),
        ];
    }
}

$closures = EvalFromCallablePersistentSpecialArrayChild::make();
$self = $closures[0];
$static = $closures[1];
$parent = $closures[2];

$first = "2";
$out = $self($first, 3) . ":" . gettype($first) . ":" . $first . "|";

$second = "4";
$out .= call_user_func_array($static, [&$second, 5]) . ":" . gettype($second) . ":" . $second . "|";

$third = "6";
$out .= $parent($third, 7) . ":" . gettype($third) . ":" . $third;

return $out;');
"#,
    );

    assert_eq!(
        out,
        "C:EvalFromCallablePersistentSpecialArrayChild:5:integer:5|\
C:EvalFromCallablePersistentSpecialArrayChild:9:integer:9|\
B:EvalFromCallablePersistentSpecialArrayChild:13:integer:13"
    );
}

/// Verifies special instance closures retain `$this` after their method scope exits.
#[test]
fn test_eval_closure_from_callable_special_instance_callables_persist_bound_receiver() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFromCallablePersistentInstanceBase {
    public function base(int &$value, int $delta): string {
        $value += $delta;
        return "B:" . get_called_class() . ":" . get_class($this) . ":" . $value;
    }
}

class EvalFromCallablePersistentInstanceChild extends EvalFromCallablePersistentInstanceBase {
    public int $base = 10;

    public function add(int &$value, int $delta): string {
        $value += $this->base + $delta;
        return "C:" . get_called_class() . ":" . get_class($this) . ":" . $value;
    }

    public function make(): array {
        return [
            Closure::fromCallable("self::add"),
            Closure::fromCallable(["static", "add"]),
            Closure::fromCallable("parent::base"),
            Closure::fromCallable(["parent", "base"]),
        ];
    }
}

$closures = (new EvalFromCallablePersistentInstanceChild())->make();
$self = $closures[0];
$static = $closures[1];
$parentString = $closures[2];
$parentArray = $closures[3];

$first = "2";
$out = $self($first, 3) . ":" . gettype($first) . ":" . $first . "|";

$second = "3";
$out .= call_user_func_array($static, [&$second, 3]) . ":" . gettype($second) . ":" . $second . "|";

$third = "4";
$out .= $parentString($third, 3) . ":" . gettype($third) . ":" . $third . "|";

$fourth = "5";
$out .= call_user_func_array($parentArray, [&$fourth, 3]) . ":" . gettype($fourth) . ":" . $fourth;

return $out;');
"#,
    );

    assert_eq!(
        out,
        "C:EvalFromCallablePersistentInstanceChild:EvalFromCallablePersistentInstanceChild:15:integer:15|\
C:EvalFromCallablePersistentInstanceChild:EvalFromCallablePersistentInstanceChild:16:integer:16|\
B:EvalFromCallablePersistentInstanceChild:EvalFromCallablePersistentInstanceChild:7:integer:7|\
B:EvalFromCallablePersistentInstanceChild:EvalFromCallablePersistentInstanceChild:8:integer:8"
    );
}

/// Verifies eval `is_callable()` supports syntax-only probes and callable-name writeback.
#[test]
fn test_eval_is_callable_supports_syntax_only_and_callable_name_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalCallableNameBox {
    public function method() {}
    private function hidden() {}
    public static function stat() {}
    public function __invoke() {}
}

$box = new EvalCallableNameBox();
$closure = function () {};

$name = "seed";
echo is_callable("\\strlen", false, $name) ? "F:" : "f:";
echo $name . "|";

$name = "seed";
echo is_callable("missing_eval_callable_name", false, $name) ? "bad:" : "M:";
echo $name . "|";

$name = "seed";
echo is_callable("missing_eval_callable_name", true, $name) ? "S:" : "s:";
echo $name . "|";

$name = "seed";
echo is_callable([$box, "method"], false, $name) ? "O:" : "o:";
echo $name . "|";

$name = "seed";
echo is_callable([$box, "hidden"], false, $name) ? "bad:" : "P:";
echo $name . "|";

$name = "seed";
echo is_callable(["NoSuchEvalCallable", "missing"], true, $name) ? "A:" : "a:";
echo $name . "|";

$name = "seed";
echo is_callable(["NoSuchEvalCallable", "missing"], false, $name) ? "bad:" : "N:";
echo $name . "|";

$name = "seed";
echo is_callable($box, false, $name) ? "I:" : "i:";
echo $name . "|";

$name = "seed";
echo is_callable($closure, false, $name) ? "C:" : "c:";
echo $name . "|";

$name = "seed";
echo is_callable(value: [$box, "method"], callable_name: $name) ? "NO:" : "no:";
echo $name . "|";

$probe = "is_callable";
$name = "seed";
echo $probe(["NoSuchDynamicCallable", "missing"], true, $name) ? "D:" : "d:";
echo $name;');
"#,
    );

    assert_eq!(
        out,
        "F:\\strlen|\
M:missing_eval_callable_name|\
S:missing_eval_callable_name|\
O:EvalCallableNameBox::method|\
P:EvalCallableNameBox::hidden|\
A:NoSuchEvalCallable::missing|\
N:NoSuchEvalCallable::missing|\
I:EvalCallableNameBox::__invoke|\
C:Closure::__invoke|\
NO:EvalCallableNameBox::method|\
D:NoSuchDynamicCallable::missing"
    );
}

/// Verifies eval callable arrays resolve `self`, `static`, and `parent` in method scope.
#[test]
fn test_eval_special_class_callable_arrays_follow_method_scope() {
    let out = compile_and_run_with_regex(
        r#"<?php
echo eval('class EvalSpecialCallableArrayBase {
    public static function parentStatic(): string {
        return "P:" . get_called_class();
    }

    public function parentInstance(): string {
        return "PI:" . get_class($this);
    }
}

class EvalSpecialCallableArrayChild extends EvalSpecialCallableArrayBase {
    public static function selfStatic(): string {
        return "S:" . get_called_class();
    }

    public function selfInstance(): string {
        return "I:" . get_class($this);
    }

    public static function mapStatic(int $value): string {
        return "MS" . $value . ":" . get_called_class();
    }

    public function mapInstance(int $value): string {
        return "MI" . $value . ":" . get_class($this);
    }

    public function keepValue(int $value): bool {
        return $value > 1;
    }

    public function reduceValue(string $carry, int $value): string {
        return $carry . $value . ":" . get_class($this) . ";";
    }

    public function walkValue(string &$value, int $key) {
        $value = $value . $key . ":" . get_class($this);
    }

    public function compareDesc(int $left, int $right): int {
        return $right - $left;
    }

    public function replaceMatch(array $matches): string {
        return "R" . $matches[0] . ":" . get_class($this);
    }

    public function run(): string {
        $out = "";
        $name = "seed";
        $out .= is_callable(["self", "selfStatic"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["self", "selfStatic"]) . "|";

        $name = "seed";
        $out .= is_callable(["static", "selfStatic"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["static", "selfStatic"]) . "|";

        $name = "seed";
        $out .= is_callable(["parent", "parentStatic"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["parent", "parentStatic"]) . "|";

        $name = "seed";
        $out .= is_callable(["self", "selfInstance"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["self", "selfInstance"]) . "|";

        $name = "seed";
        $out .= is_callable(["parent", "parentInstance"], false, $name) ? "Y:" . $name . ":" : "N:";
        $out .= call_user_func(["parent", "parentInstance"]) . "|";

        $fromInstance = Closure::fromCallable(["self", "selfInstance"]);
        $out .= $fromInstance() . "|";

        $fromStatic = Closure::fromCallable(["parent", "parentStatic"]);
        $out .= $fromStatic();

        $out .= "|" . implode(",", array_map(["static", "mapStatic"], [3]));
        $out .= "|" . implode(",", array_map(["self", "mapInstance"], [4]));
        $out .= "|" . implode(",", array_filter([1, 2], ["self", "keepValue"]));
        $out .= "|" . array_reduce([1, 2], ["self", "reduceValue"], "");

        $walk = ["x"];
        array_walk($walk, ["self", "walkValue"]);
        $out .= "|" . $walk[0];

        $sort = [1, 3, 2];
        usort($sort, ["self", "compareDesc"]);
        $out .= "|" . implode(",", $sort);

        $out .= "|" . preg_replace_callback("/a/", ["self", "replaceMatch"], "a");

        try {
            $direct = ["self", "selfStatic"];
            $out .= "|" . $direct();
        } catch (Error $e) {
            $out .= "|" . get_class($e) . ":" . $e->getMessage();
        }

        return $out;
    }
}

return (new EvalSpecialCallableArrayChild())->run();');
"#,
    );

    assert_eq!(
        out,
        "Y:self::selfStatic:S:EvalSpecialCallableArrayChild|\
Y:static::selfStatic:S:EvalSpecialCallableArrayChild|\
Y:parent::parentStatic:P:EvalSpecialCallableArrayChild|\
Y:self::selfInstance:I:EvalSpecialCallableArrayChild|\
Y:parent::parentInstance:PI:EvalSpecialCallableArrayChild|\
I:EvalSpecialCallableArrayChild|\
P:EvalSpecialCallableArrayChild|\
MS3:EvalSpecialCallableArrayChild|\
MI4:EvalSpecialCallableArrayChild|\
2|\
1:EvalSpecialCallableArrayChild;2:EvalSpecialCallableArrayChild;|\
x0:EvalSpecialCallableArrayChild|\
3,2,1|\
Ra:EvalSpecialCallableArrayChild|\
Error:Class \"self\" not found"
    );
}

/// Verifies eval special-class callable arrays preserve by-reference writeback.
#[test]
fn test_eval_special_class_callable_arrays_preserve_by_ref_writeback() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalSpecialArrayRefBase {
    public static function bump(int &$value, int $delta): int {
        $value += $delta;
        return $value;
    }
}

class EvalSpecialArrayRefChild extends EvalSpecialArrayRefBase {
    public int $base = 10;

    public function add(int &$value, int $delta): int {
        $value += $this->base + $delta;
        return $value;
    }

    public function run(): string {
        $out = "";

        $first = "2";
        $out .= call_user_func_array(["self", "add"], [&$first, 3]) . ":" . gettype($first) . ":" . $first . "|";

        $second = "4";
        $static = Closure::fromCallable(["static", "add"]);
        $out .= $static($second, 5) . ":" . gettype($second) . ":" . $second . "|";

        $third = "6";
        $out .= call_user_func_array(["parent", "bump"], [&$third, 7]) . ":" . gettype($third) . ":" . $third . "|";

        $fourth = "8";
        $parent = Closure::fromCallable(["parent", "bump"]);
        $out .= $parent($fourth, 9) . ":" . gettype($fourth) . ":" . $fourth;

        return $out;
    }
}

return (new EvalSpecialArrayRefChild())->run();');
"#,
    );

    assert_eq!(
        out,
        "15:integer:15|19:integer:19|13:integer:13|17:integer:17"
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

/// Verifies `call_user_func()` invokes `Closure::fromCallable()` targets by value.
#[test]
fn test_eval_closure_from_callable_call_user_func_uses_by_value_args() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_call_user_func_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalFromCallableCallUserFuncRefBox {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$function = Closure::fromCallable("eval_from_callable_call_user_func_ref_add");
$a = "2";
echo call_user_func($function, $a, 3) . ":" . gettype($a) . ":" . $a . "|";

$box = new EvalFromCallableCallUserFuncRefBox();
$method = Closure::fromCallable([$box, "bump"]);
$b = "4";
echo call_user_func($method, $b, 5) . ":" . gettype($b) . ":" . $b . "|";

$invoke = Closure::fromCallable($box);
$c = "6";
echo call_user_func($invoke, $c, 7) . ":" . gettype($c) . ":" . $c . "|";

$static = Closure::fromCallable(["EvalFromCallableCallUserFuncRefBox", "add"]);
$d = "8";
return call_user_func($static, $d, 9) . ":" . gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(out, "5:string:2|19:string:4|23:string:6|17:string:8");
}

/// Verifies `call_user_func_array()` degrades non-reference `Closure::fromCallable()` args by value.
#[test]
fn test_eval_closure_from_callable_call_user_func_array_degrades_non_ref_args() {
    let out = compile_and_run(
        r#"<?php
function eval_from_callable_call_user_func_array_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalFromCallableCallUserFuncArrayRefBox {
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

echo eval('$function = Closure::fromCallable("eval_from_callable_call_user_func_array_ref_add");
$a = "2";
$aArgs = [$a, 3];
echo call_user_func_array($function, $aArgs) . ":" . gettype($a) . ":" . $a . ":" . gettype($aArgs[0]) . ":" . $aArgs[0] . "|";

$b = "4";
$bArgs = [&$b, 5];
echo call_user_func_array($function, $bArgs) . ":" . gettype($b) . ":" . $b . ":" . gettype($bArgs[0]) . ":" . $bArgs[0] . "|";

$box = new EvalFromCallableCallUserFuncArrayRefBox();
$method = Closure::fromCallable([$box, "bump"]);
$c = "6";
$cArgs = [$c, 7];
echo call_user_func_array($method, $cArgs) . ":" . gettype($c) . ":" . $c . ":" . gettype($cArgs[0]) . ":" . $cArgs[0] . "|";

$d = "8";
$dArgs = [&$d, 9];
echo call_user_func_array($method, $dArgs) . ":" . gettype($d) . ":" . $d . ":" . gettype($dArgs[0]) . ":" . $dArgs[0] . "|";

$static = Closure::fromCallable(["EvalFromCallableCallUserFuncArrayRefBox", "add"]);
$e = "10";
$eArgs = [$e, 11];
return call_user_func_array($static, $eArgs) . ":" . gettype($e) . ":" . $e . ":" . gettype($eArgs[0]) . ":" . $eArgs[0];');
"#,
    );

    assert_eq!(
        out,
        "5:string:2:string:2|9:integer:9:integer:9|23:string:6:string:6|27:integer:27:integer:27|21:string:10:string:10"
    );
}

/// Verifies `Closure::fromCallable()` values can cross eval into AOT callable parameters.
#[test]
fn test_eval_closure_from_callable_values_pass_to_aot_callable_params() {
    let out = compile_and_run(
        r##"<?php
function eval_closure_bridge_suffix(string $value): string {
    return $value . "!";
}

class EvalClosureBridgeTarget {
    public static function suffix(string $value): string {
        return $value . "?";
    }

    public function instanceSuffix(string $value): string {
        return $value . "~";
    }

    public function __invoke(string $value): string {
        return $value . "#";
    }
}

class EvalClosureBridgeBox {
    public $value = "";

    public function __construct(callable $callback) {
        $this->value = $callback("C");
    }

    public function apply(callable $callback) {
        return $callback("M");
    }

    public static function applyStatic(callable $callback) {
        return $callback("S");
    }
}

echo eval('$target = new EvalClosureBridgeTarget();
$cases = [
    Closure::fromCallable("eval_closure_bridge_suffix"),
    Closure::fromCallable([EvalClosureBridgeTarget::class, "suffix"]),
    Closure::fromCallable("EvalClosureBridgeTarget::suffix"),
    Closure::fromCallable([$target, "instanceSuffix"]),
    Closure::fromCallable($target),
];
$out = [];
foreach ($cases as $callback) {
    $box = new EvalClosureBridgeBox($callback);
    $out[] = $box->value . ":" . $box->apply($callback) . ":" .
        EvalClosureBridgeBox::applyStatic($callback);
}
return implode("|", $out);');
"##,
    );

    assert_eq!(
        out,
        "C!:M!:S!|C?:M?:S?|C?:M?:S?|C~:M~:S~|C#:M#:S#"
    );
}

/// An eval closure with a boolean result can be retained by an AOT object property and invoked
/// after the eval frame returns, covering callable descriptor metadata across that ownership edge.
#[test]
fn test_eval_bool_closure_stored_in_aot_property_invokes_after_eval_returns() {
    let out = compile_and_run(
        r#"<?php
class EvalBoolValidator {
    private ?\Closure $validator = null;
    public function setValidator(?\Closure $validator): void { $this->validator = $validator; }
    public function validate(array $values): bool { return ($this->validator)($values); }
}
$validator = new EvalBoolValidator();
eval('class EvalBoolValidatorFactory { public function register(EvalBoolValidator $validator): void { $validator->setValidator(static fn (array $values): bool => !in_array("blocked", $values, true)); } }');
$factory = eval('return new EvalBoolValidatorFactory();');
$factory->register($validator);
echo $validator->validate(['ready']) ? 'yes' : 'no';
"#,
    );

    assert_eq!(out, "yes");
}

/// Multiple Eval closures with different return types remain callable after an AOT builder stores
/// them in separate Closure-typed properties and the declaring eval frame has returned.
#[test]
fn test_eval_bool_and_array_closures_survive_aot_builder_storage() {
    let out = compile_and_run(
        r#"<?php
class EvalClosureBuilder {
    private ?\Closure $predicate = null;
    private ?\Closure $normalizer = null;
    public function setCallbacks(\Closure $predicate, \Closure $normalizer): void {
        $this->predicate = $predicate;
        $this->normalizer = $normalizer;
    }
    public function normalize(array $values): array {
        if (!($this->predicate)($values)) {
            return [];
        }
        return ($this->normalizer)($values);
    }
}
$builder = new EvalClosureBuilder();
eval('class EvalClosureBuilderFactory { public function configure(EvalClosureBuilder $builder): void { $builder->setCallbacks(static fn (array $values): bool => !in_array("blocked", $values, true), static fn (array $values): array => array_merge(["default"], $values)); } }');
$factory = eval('return new EvalClosureBuilderFactory();');
$factory->configure($builder);
echo implode(",", $builder->normalize(["ready"]));
"#,
    );

    assert_eq!(out, "default,ready");
}

/// Eval closures returned from a builder method and stored in an AOT array remain callable during
/// later array traversal, covering the callback-array path used by normalizers and validators.
#[test]
fn test_eval_closures_stored_in_aot_array_invoke_after_eval_returns() {
    let out = compile_and_run(
        r#"<?php
class EvalClosureList {
    private array $callbacks = [];
    public function add(\Closure $callback): void { $this->callbacks[] = $callback; }
    public function validate(array $values): bool {
        foreach ($this->callbacks as $callback) {
            if (!$callback($values)) {
                return false;
            }
        }
        return true;
    }
}
$list = new EvalClosureList();
eval('class EvalClosureListFactory { public function configure(EvalClosureList $list): void { $list->add(static fn (array $values): bool => !in_array("blocked", $values, true)); $list->add(static fn (array $values): bool => is_array($values)); } }');
$factory = eval('return new EvalClosureListFactory();');
$factory->configure($list);
echo $list->validate(['ready']) ? 'yes' : 'no';
"#,
    );

    assert_eq!(out, "yes");
}

/// Mirrors the nullable Closure parameter and stored-predicate flow of a generic fluent builder.
#[test]
fn test_eval_closure_through_nullable_aot_builder_parameter_invokes_after_eval_returns() {
    let out = compile_and_run(
        r#"<?php
class EvalNullableClosureBuilder {
    public ?\Closure $ifPart = null;
    public string $allowedTypes = '';
    public function ifTrue(?\Closure $closure = null): static {
        $this->ifPart = $closure ?? static fn ($value) => true === $value;
        $this->allowedTypes = $closure ? '*' : 'bool';
        return $this;
    }
    public function evaluate(mixed $value): bool { return ($this->ifPart)($value); }
}
$builder = new EvalNullableClosureBuilder();
eval('class EvalNullableClosureFactory { public function configure(EvalNullableClosureBuilder $builder): void { $builder->ifTrue(static fn (array $values): bool => !in_array("blocked", $values, true)); } }');
$factory = eval('return new EvalNullableClosureFactory();');
$factory->configure($builder);
echo ($builder->evaluate(['ready']) ? 'yes' : 'no') . ':';
$defaultBuilder = new EvalNullableClosureBuilder();
$defaultBuilder->ifTrue(null);
echo $defaultBuilder->evaluate(true) ? 'yes' : 'no';
"#,
    );

    assert_eq!(out, "yes:yes");
}

/// Verifies eval dynamic callable descriptors preserve AOT caller by-ref variables.
#[test]
fn test_eval_dynamic_callable_params_write_back_aot_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
function eval_dynamic_callable_ref_add(int &$value, int $delta): int {
    $value = $value + $delta;
    return $value;
}

class EvalDynamicCallableRefBridgeBox {
    public string $value = "";

    public function __construct(callable $callback) {
        $value = 2;
        $this->value = $callback($value, 3) . ":" . gettype($value) . ":" . $value;
    }

    public function apply(callable $callback): string {
        $value = 4;
        return $callback($value, 5) . ":" . gettype($value) . ":" . $value;
    }

    public static function applyStatic(callable $callback): string {
        $value = 6;
        return $callback($value, 7) . ":" . gettype($value) . ":" . $value;
    }
}

echo eval('$callback = Closure::fromCallable("eval_dynamic_callable_ref_add");
$box = new EvalDynamicCallableRefBridgeBox($callback);
return $box->value . "|" . $box->apply($callback) . "|" .
    EvalDynamicCallableRefBridgeBox::applyStatic($callback);');
"#,
    );

    assert_eq!(out, "5:integer:5|9:integer:9|13:integer:13");
}

/// Verifies eval dynamic callable descriptors preserve AOT mixed by-ref slots.
#[test]
fn test_eval_dynamic_callable_params_write_back_aot_mixed_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
function eval_dynamic_callable_mixed_replace(mixed &$value, mixed $next): string {
    $value = $next;
    return gettype($value) . ":" . $value;
}

class EvalDynamicCallableMixedRefBridgeBox {
    public string $value = "";

    public function __construct(callable $callback, mixed $seed) {
        $value = $seed;
        $this->value = $callback($value, "ctor") . ":" . gettype($value) . ":" . $value;
    }

    public function apply(callable $callback, mixed $seed): string {
        $value = $seed;
        return $callback($value, "method") . ":" . gettype($value) . ":" . $value;
    }

    public static function applyStatic(callable $callback, mixed $seed): string {
        $value = $seed;
        return $callback($value, "static") . ":" . gettype($value) . ":" . $value;
    }
}

echo eval('$callback = Closure::fromCallable("eval_dynamic_callable_mixed_replace");
$box = new EvalDynamicCallableMixedRefBridgeBox($callback, 2);
return $box->value . "|" . $box->apply($callback, 4) . "|" .
    EvalDynamicCallableMixedRefBridgeBox::applyStatic($callback, 6);');
"#,
    );

    assert_eq!(
        out,
        "string:ctor:string:ctor|string:method:string:method|string:static:string:static"
    );
}

/// Verifies a Throwable from a dynamic eval callable crosses an AOT method and native catch.
#[test]
fn test_eval_dynamic_callable_throwable_crosses_aot_method_to_native_catch() {
    let out = compile_and_run(
        r#"<?php
function eval_dynamic_callable_throwing_aot(): int {
    throw new RuntimeException("eval callable boom");
}

class EvalDynamicCallableThrowableBridgeBox {
    public function invoke(callable $callback): int {
        return $callback();
    }
}

$box = new EvalDynamicCallableThrowableBridgeBox();
try {
    eval('return $box->invoke("eval_dynamic_callable_throwing_aot");');
    echo "bad";
} catch (RuntimeException $caught) {
    echo get_class($caught) . ":" . $caught->getMessage();
}
"#,
    );

    assert_eq!(out, "RuntimeException:eval callable boom");
}

/// Verifies `Closure::call()` rebinds method closures but passes later args by value.
#[test]
fn test_eval_closure_from_callable_call_rebinds_targets_and_uses_by_value_args() {
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

    assert_eq!(out, "25:string:2|29:string:4|F|S");
}

/// Verifies `Closure::call()` preserves named argument mapping while using by-value args.
#[test]
fn test_eval_closure_from_callable_call_named_args_use_by_value_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalFromCallableCallNamedBox {
    public int $base = 0;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

echo eval('$original = new EvalFromCallableCallNamedBox();
$original->base = 10;
$bound = new EvalFromCallableCallNamedBox();
$bound->base = 20;

$method = Closure::fromCallable([$original, "bump"]);
$a = "2";
echo $method->call(newThis: $bound, delta: 3, value: $a) . ":" . gettype($a) . ":" . $a . "|";

$invoke = Closure::fromCallable($original);
$b = "4";
return $invoke->call(newThis: $bound, value: $b, delta: 5) . ":" . gettype($b) . ":" . $b;');
"#,
    );

    assert_eq!(out, "25:string:2|29:string:4");
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

/// Verifies static `Closure::bind()` persists rebinding for `fromCallable()` targets.
#[test]
fn test_eval_static_closure_bind_from_callable_persists_method_and_invokable_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalStaticFromCallableBindBox {
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

echo eval('$original = new EvalStaticFromCallableBindBox();
$original->base = 10;
$bound = new EvalStaticFromCallableBindBox();
$bound->base = 20;

$rawMethod = Closure::fromCallable([$original, "bump"]);
$method = Closure::bind(closure: $rawMethod, newThis: $bound);
$a = "2";
echo $method(delta: 3, value: $a) . ":" . gettype($a) . ":" . $a . "|";

$rawInvoke = Closure::fromCallable($original);
$invoke = Closure::bind($rawInvoke, $bound);
$b = "4";
echo call_user_func_array($invoke, ["delta" => 5, "value" => &$b]) .
    ":" . gettype($b) . ":" . $b . "|";

$static = Closure::fromCallable(["EvalStaticFromCallableBindBox", "add"]);
echo is_null(Closure::bind($static, $bound)) ? "S" : "s";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|S");
}

/// Verifies eval `Closure::bind()` can bind inherited method closures to compatible subclasses.
#[test]
fn test_eval_closure_bind_from_callable_accepts_inherited_eval_method_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalInheritedBindBase {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

class EvalInheritedBindChild extends EvalInheritedBindBase {}

class EvalInheritedBindOverrideChild extends EvalInheritedBindBase {
    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta + 100;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta + 100;
        return $value;
    }
}

$base = new EvalInheritedBindBase();
$child = new EvalInheritedBindChild();
$child->base = 20;

$method = Closure::fromCallable([$base, "bump"])->bindTo($child);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$invoke = Closure::fromCallable($base)->bindTo($child);
$b = "4";
echo $invoke($b, 5) . ":" . gettype($b) . ":" . $b . "|";

$override = new EvalInheritedBindOverrideChild();
echo is_null(Closure::fromCallable([$override, "bump"])->bindTo($base)) ? "M" : "m";
echo is_null(Closure::fromCallable($override)->bindTo($base)) ? "I" : "i";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|MI");
}

/// Verifies eval `Closure::bind()` can bind inherited generated/AOT method closures to subclasses.
#[test]
fn test_eval_closure_bind_from_callable_accepts_inherited_aot_method_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalAotInheritedBindBase {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

class EvalAotInheritedBindChild extends EvalAotInheritedBindBase {}

echo eval('$base = new EvalAotInheritedBindBase();
$child = new EvalAotInheritedBindChild();
$child->base = 20;

$method = Closure::fromCallable([$base, "bump"])->bindTo($child);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$invoke = Closure::fromCallable($base)->bindTo($child);
$b = "4";
echo $invoke($b, 5) . ":" . gettype($b) . ":" . $b;');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29");
}

/// Verifies eval receives a boxed concrete object from an AOT method declared with an interface return.
#[test]
fn test_eval_aot_method_interface_return_boxes_concrete_object() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotReturnContract {}

class EvalAotReturnConcrete implements EvalAotReturnContract {
    public function marker(): string {
        return "ok";
    }
}

class EvalAotReturnOwner {
    public function resolve(): EvalAotReturnContract {
        return new EvalAotReturnConcrete();
    }
}

echo eval('$owner = new EvalAotReturnOwner();
$value = $owner->resolve();
echo $value instanceof EvalAotReturnContract ? $value->marker() : "bad";');
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies eval adapts a reflected callable before an AOT interface-returning method uses it.
#[test]
fn test_eval_aot_reflected_callable_method_returns_interface_object() {
    let out = compile_and_run(
        r#"<?php
interface EvalReflectionResolver {
    public function resolve(): string;
}

class EvalReflectionClosureResolver implements EvalReflectionResolver {
    public function __construct(
        private readonly Closure $callable,
        private readonly Closure $arguments,
    ) {}

    public function resolve(): string {
        return ($this->callable)(...($this->arguments)());
    }
}

class EvalReflectionRuntime {
    public function getResolver(
        callable $callable,
        ?ReflectionFunction $reflector = null,
    ): EvalReflectionResolver {
        $callable = $callable(...);
        $reflector ??= new ReflectionFunction($callable);
        $parameters = [];
        $arguments = function () use ($parameters): array { return $parameters; };

        return new EvalReflectionClosureResolver($callable, $arguments);
    }
}

echo eval('$runtime = new EvalReflectionRuntime();
$callback = static function (): string { return "ok"; };
echo $runtime->getResolver($callback)->resolve();');
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies first-class eval method closures bind inherited method targets to subclasses.
#[test]
fn test_eval_first_class_closure_bind_accepts_inherited_eval_method_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalFirstClassInheritedBindBase {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

class EvalFirstClassInheritedBindChild extends EvalFirstClassInheritedBindBase {}

class EvalFirstClassInheritedBindOverrideChild extends EvalFirstClassInheritedBindBase {
    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta + 100;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta + 100;
        return $value;
    }
}

$base = new EvalFirstClassInheritedBindBase();
$child = new EvalFirstClassInheritedBindChild();
$child->base = 20;

$methodSource = $base->bump(...);
$method = $methodSource->bindTo($child);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$invokeSource = $base(...);
$invoke = $invokeSource->bindTo($child);
$b = "4";
echo $invoke($b, 5) . ":" . gettype($b) . ":" . $b . "|";

$callMethod = $base->bump(...);
$c = "6";
echo is_null($callMethod->call($child, $c, 7)) ? "C:" : "c:";
echo gettype($c) . ":" . $c . "|";

$callInvoke = $base(...);
$d = "8";
echo is_null($callInvoke->call($child, $d, 9)) ? "I:" : "i:";
echo gettype($d) . ":" . $d . "|";

$override = new EvalFirstClassInheritedBindOverrideChild();
$overrideMethod = $override->bump(...);
echo is_null($overrideMethod->bindTo($base)) ? "M" : "m";
$overrideInvoke = $override(...);
echo is_null($overrideInvoke->bindTo($base)) ? "I" : "i";');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|C:string:6|I:string:8|MI");
}

/// Verifies first-class AOT method closures bind inherited method targets to subclasses.
#[test]
fn test_eval_first_class_closure_bind_accepts_inherited_aot_method_targets() {
    let out = compile_and_run(
        r#"<?php
class EvalAotFirstClassInheritedBindBase {
    public int $base = 10;

    public function bump(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->base + $delta;
        return $value;
    }
}

class EvalAotFirstClassInheritedBindChild extends EvalAotFirstClassInheritedBindBase {}

echo eval('$base = new EvalAotFirstClassInheritedBindBase();
$child = new EvalAotFirstClassInheritedBindChild();
$child->base = 20;

$methodSource = $base->bump(...);
$method = $methodSource->bindTo($child);
$a = "2";
echo $method($a, 3) . ":" . gettype($a) . ":" . $a . "|";

$invokeSource = $base(...);
$invoke = $invokeSource->bindTo($child);
$b = "4";
echo $invoke($b, 5) . ":" . gettype($b) . ":" . $b . "|";

$callMethod = $base->bump(...);
$c = "6";
echo is_null($callMethod->call($child, $c, 7)) ? "C:" : "c:";
echo gettype($c) . ":" . $c . "|";

$callInvoke = $base(...);
$d = "8";
echo is_null($callInvoke->call($child, $d, 9)) ? "I:" : "i:";
echo gettype($d) . ":" . $d;');
"#,
    );

    assert_eq!(out, "25:integer:25|29:integer:29|C:string:6|I:string:8");
}

/// Verifies binding function `fromCallable()` closures returns callable closures.
#[test]
fn test_eval_closure_bind_from_callable_function_targets_remain_callable() {
    let out = compile_and_run(
        r#"<?php
function eval_bind_from_callable_function_target(string $value): string {
    return "A:" . $value;
}

class EvalBindFromCallableFunctionBox {}

echo eval('function eval_declared_bind_from_callable_function_target(string $value): string {
    return "E:" . $value;
}

$box = new EvalBindFromCallableFunctionBox();

$aot = Closure::fromCallable("eval_bind_from_callable_function_target");
$aotBoundTo = $aot->bindTo($box);
echo is_object($aotBoundTo) ? get_class($aotBoundTo) . ":" . $aotBoundTo("x") : "bad";
echo "|";

$aotBound = Closure::bind(closure: $aot, newThis: $box);
echo is_object($aotBound) ? get_class($aotBound) . ":" . $aotBound("y") : "bad";
echo "|";

$eval = Closure::fromCallable("eval_declared_bind_from_callable_function_target");
$evalBoundTo = $eval->bindTo($box);
echo is_object($evalBoundTo) ? get_class($evalBoundTo) . ":" . $evalBoundTo("u") : "bad";
echo "|";

$evalBound = Closure::bind($eval, $box);
return is_object($evalBound) ? get_class($evalBound) . ":" . $evalBound("v") : "bad";');
"#,
    );

    assert_eq!(out, "Closure:A:x|Closure:A:y|Closure:E:u|Closure:E:v");
}

/// Verifies function `fromCallable()` closures reject explicit scope rebinding.
#[test]
fn test_eval_closure_bind_from_callable_function_targets_reject_explicit_scope() {
    let out = compile_and_run(
        r#"<?php
function eval_bind_from_callable_scope_function_target(string $value): string {
    return "A:" . $value;
}

class EvalBindFromCallableScopeBox {}

echo eval('function eval_declared_bind_from_callable_scope_function_target(string $value): string {
    return "E:" . $value;
}

$box = new EvalBindFromCallableScopeBox();

$aot = Closure::fromCallable("eval_bind_from_callable_scope_function_target");
$aotNullScope = $aot->bindTo($box, null);
echo is_object($aotNullScope) ? $aotNullScope("x") : "bad";
echo "|";
$aotStaticScope = Closure::bind($aot, $box, "static");
echo is_object($aotStaticScope) ? $aotStaticScope("y") : "bad";
echo "|";
echo is_null($aot->bindTo($box, "EvalBindFromCallableScopeBox")) ? "a" : "A";
echo "|";
echo is_null(Closure::bind($aot, null, "EvalBindFromCallableScopeBox")) ? "b" : "B";
echo "|";

$eval = Closure::fromCallable("eval_declared_bind_from_callable_scope_function_target");
$evalNullScope = Closure::bind($eval, $box, null);
echo is_object($evalNullScope) ? $evalNullScope("u") : "bad";
echo "|";
$evalStaticScope = $eval->bindTo($box, "static");
echo is_object($evalStaticScope) ? $evalStaticScope("v") : "bad";
echo "|";
echo is_null($eval->bindTo($box, "EvalBindFromCallableScopeBox")) ? "e" : "E";
echo "|";
return is_null(Closure::bind($eval, null, "EvalBindFromCallableScopeBox")) ? "f" : "F";');
"#,
    );

    assert_eq!(out, "A:x|A:y|a|b|E:u|E:v|e|f");
}

/// Verifies an eval Closure crossing an AOT callable parameter retains Reflection metadata.
#[test]
fn test_eval_callable_adapter_reflection_parameters_keep_concrete_objects() {
    let out = compile_and_run(
        r#"<?php
class EvalCallableReflectionProbe {
    public function inspect(callable $callable, ?ReflectionFunction $reflector = null): void {
        $reflection = $reflector ?? new ReflectionFunction($callable);
        $parameters = $reflection->getParameters();
        echo count($parameters) . ':';
        foreach ($parameters as $parameter) {
            echo $parameter->getType()->getName();
        }
    }
}

$runtime = eval('return new EvalCallableReflectionProbe();');
$callback = eval('return static function (array $context): object { return new stdClass(); };');
$runtime->inspect($callback);
"#,
    );
    assert_eq!(out, "1:array");
}

/// Verifies ReflectionFunction reports retained metadata for Closure callables.
#[test]
fn test_eval_reflection_function_reports_from_callable_closure_metadata() {
    let out = compile_and_run(
        r#"<?php
function eval_reflect_closure_meta_aot_function(): string {
    return "aot";
}

class EvalReflectClosureMetaAotBox {
    public function method(): string { return "method"; }
    public static function stat(): string { return "static"; }
    public function __invoke(): string { return "invoke"; }
}

echo eval('function eval_reflect_closure_meta_dump(string $label, $closure): string {
    try {
        $ref = new ReflectionFunction($closure);
        $this = $ref->getClosureThis();
        $scope = $ref->getClosureScopeClass();
        $called = $ref->getClosureCalledClass();
        return $label . ":" .
            (is_object($this) ? get_class($this) : "null") . ":" .
            ($scope ? $scope->getName() : "null") . ":" .
            ($called ? $called->getName() : "null") . ":" .
            ($ref->isStatic() ? "S" : "s");
    } catch (Throwable $e) {
        return $label . ":ERR:" . get_class($e) . ":" . $e->getMessage();
    }
}

function eval_reflect_closure_meta_eval_function(): string {
    return "eval";
}

class EvalReflectClosureMetaEvalBox {
    public function method(): string { return "method"; }
    public static function stat(): string { return "static"; }
    public function __invoke(): string { return "invoke"; }
}

$aot = new EvalReflectClosureMetaAotBox();
$eval = new EvalReflectClosureMetaEvalBox();

$out = [];
$out[] = eval_reflect_closure_meta_dump(
    "aot-fn",
    Closure::bind(Closure::fromCallable("eval_reflect_closure_meta_aot_function"), $aot)
);
$out[] = eval_reflect_closure_meta_dump(
    "eval-fn",
    Closure::bind(Closure::fromCallable("eval_reflect_closure_meta_eval_function"), $eval)
);
$out[] = eval_reflect_closure_meta_dump("aot-method", Closure::fromCallable([$aot, "method"]));
$out[] = eval_reflect_closure_meta_dump("eval-method", Closure::fromCallable([$eval, "method"]));
$out[] = eval_reflect_closure_meta_dump("aot-invoke", Closure::fromCallable($aot));
$out[] = eval_reflect_closure_meta_dump(
    "eval-static",
    Closure::fromCallable(["EvalReflectClosureMetaEvalBox", "stat"])
);
return implode("|", $out);');
"#,
    );

    assert_eq!(
        out,
        "aot-fn:EvalReflectClosureMetaAotBox:Closure:EvalReflectClosureMetaAotBox:s|\
eval-fn:EvalReflectClosureMetaEvalBox:Closure:EvalReflectClosureMetaEvalBox:s|\
aot-method:EvalReflectClosureMetaAotBox:EvalReflectClosureMetaAotBox:EvalReflectClosureMetaAotBox:s|\
eval-method:EvalReflectClosureMetaEvalBox:EvalReflectClosureMetaEvalBox:EvalReflectClosureMetaEvalBox:s|\
aot-invoke:EvalReflectClosureMetaAotBox:EvalReflectClosureMetaAotBox:EvalReflectClosureMetaAotBox:s|\
eval-static:null:EvalReflectClosureMetaEvalBox:EvalReflectClosureMetaEvalBox:S"
    );
}


/// An eval subclass that redeclares an inherited property default is seen with that default by
/// its compiled parent's methods, which read the native slot. Symfony's interpreted
/// `CheckTypeDeclarationsPass` redeclares `$skipScalars = true` over the compiled
/// `AbstractRecursivePass`, whose `processValue()` kept reading `false`.
#[test]
fn test_eval_subclass_redeclared_default_reaches_compiled_parent() {
    let out = compile_and_run(
        r#"<?php
abstract class Base {
    protected bool $flag = false;
    protected ?string $name = null;
    public function show(): string { return var_export($this->flag, true) . ',' . var_export($this->name, true); }
}
if (false) { new class extends Base {}; }
eval('class Child extends Base { protected bool $flag = true; protected ?string $name = "child"; }');
echo (new Child())->show();
"#,
    );
    assert_eq!(out, "true,'child'");
}

/// A throw inside a compiled first-class callable called from eval code still reaches the
/// interpreted `catch`: the invoker's boundary applies when the interpreter is the caller.
#[test]
fn test_eval_catches_throw_from_compiled_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
class NotFound extends InvalidArgumentException {}
function thrower(string $n): string { throw new NotFound("nf:$n"); }
$GLOBALS['f'] = thrower(...);
eval('try { ($GLOBALS["f"])("x"); echo "no throw"; } catch (NotFound $e) { echo "caught ", $e->getMessage(); }');
"#,
    );
    assert_eq!(out, "caught nf:x");
}

/// Every interpreted spelling that reaches a compiled callable descriptor keeps a throw catchable
/// by the interpreted `try`, while a compiled `$f()` over the same descriptor still unwinds to
/// its compiled `catch`. The descriptor invoker consumes the interpreter's boundary request: it
/// was raised but never read, so the throw unwound natively past the eval and died uncaught.
#[test]
fn test_eval_catches_throw_from_compiled_callable_descriptors() {
    let out = compile_and_run(
        r#"<?php
class NotFoundAgain extends InvalidArgumentException {}
function thrower_again(string $n): string { throw new NotFoundAgain("nf:$n"); }
$g = thrower_again(...);
$h = fn ($n) => thrower_again($n);
eval('try { $g("b"); echo "no throw"; } catch (NotFoundAgain $e) { echo "caught ", $e->getMessage(), "|"; }');
eval('try { call_user_func($g, "c"); echo "no throw"; } catch (NotFoundAgain $e) { echo "caught ", $e->getMessage(), "|"; }');
eval('try { $h("d"); echo "no throw"; } catch (NotFoundAgain $e) { echo "caught ", $e->getMessage(), "|"; }');
try { $g("e"); } catch (NotFoundAgain $e) { echo "compiled ", $e->getMessage(), "|"; }
eval('try { try { $g("f"); } finally { echo "finally|"; } } catch (Exception $e) { echo "outer ", $e->getMessage(); }');
"#,
    );
    assert_eq!(
        out,
        "caught nf:b|caught nf:c|caught nf:d|compiled nf:e|finally|outer nf:f"
    );
}

/// A function declared inside `if (!function_exists(...))` is callable from eval under its public
/// name once its declaration ran, and one whose guard stayed false does not exist for eval, which
/// may then declare the name itself.
#[test]
fn test_eval_callback_builtins_take_compiled_closures() {
    // A compiled closure reaches interpreted code as its callable descriptor. Only `$f()` knew
    // that shape; every callback builtin normalized it by its string form, found no function of
    // that name, and refused it -- Laravel's interpreted `Arr::map()` is `array_map($callback,
    // $array, $keys)` over a compiled closure.
    let out = compile_and_run(
        r#"<?php
$inc = fn ($v) => $v + 1;
$cmp = function ($a, $b) { return $a <=> $b; };
$pair = fn ($v, $k) => "$k=$v";
$named = fn (int $a, int $b = 10) => $a * $b;
eval('echo implode(",", array_map($inc, [1, 2])), "\n";
$list = [3, 1, 2];
usort($list, $cmp);
echo implode(",", $list), "\n";
echo implode(",", array_map($pair, ["x" => 1, "y" => 2], ["x", "y"])), "\n";
echo call_user_func($inc, 4), " ", call_user_func_array($named, [2]), " ", call_user_func_array($named, ["b" => 3, "a" => 2]), "\n";
var_dump(is_callable($inc));');
"#,
    );
    assert_eq!(out, "2,3\n1,2,3\nx=1,y=2\n5 20 6\nbool(true)\n");
}

#[test]
fn test_eval_passes_surplus_arguments_to_compiled_closures_and_functions() {
    // php hands a user function or closure surplus positional arguments without complaint;
    // Laravel's `Arr::mapWithKeys()` calls every callback as `$callback($value, $key)`, and a
    // compiled one-parameter closure was refused outright when interpreted code called it.
    let out = compile_and_run(
        r#"<?php
class M { public $p = "x"; function run() { $f = function ($package) { return [$this->p . $package => 1]; }; return eval('return $f("a", "b");'); } }
var_dump((new M)->run());
function one($a) { return "one:$a"; }
$g = fn ($v) => "g:$v";
eval('echo $g(1, 2), " ", one(3, 4), "\n";');
"#,
    );
    assert_eq!(out, "array(1) {\n  [\"xa\"]=>\n  int(1)\n}\ng:1 one:3\n");
}

#[test]
fn test_eval_calls_conditionally_declared_compiled_function() {
    let out = compile_and_run(
        r#"<?php
if (!function_exists('cond_value')) {
    function cond_value($value, ...$args) { return $value instanceof Closure ? $value(...$args) : $value; }
}
if (function_exists('strlen') === false) {
    function strlen_never($x) { return -1; }
}
echo cond_value(3), "|";
eval('echo cond_value(5), "|", cond_value(fn () => 6), "|", cond_value(fn ($a) => $a * 2, 7), "|";');
eval('echo function_exists("strlen_never") ? "declared" : "absent", "|";');
eval('function strlen_never($x) { return "eval"; } echo strlen_never(1);');
"#,
    );
    assert_eq!(out, "3|5|6|14|absent|eval");
}

/// An interpreted callable handed to a compiled `Closure`/`callable` parameter becomes a
/// descriptor at the bridge: closures, function names, array and static-string callables, through
/// a variadic spread, and through a `mixed` parameter narrowed by `instanceof Closure`.
#[test]
fn test_eval_callables_reach_compiled_callable_parameters() {
    let out = compile_and_run(
        r#"<?php
function call_typed(Closure $value) { return $value(); }
function call_callable(callable $value) { return $value(); }
function call_spread($value, ...$args) { return $value(...$args); }
function call_narrowed($value) { return $value instanceof Closure ? $value() : $value; }
function named_five() { return 5; }
class Box { public function nine() { return 9; } public static function ten() { return 10; } }
echo call_spread(fn () => 1), call_narrowed(2), "|";
eval('echo call_typed(fn () => 3), call_callable(function () { return 4; }), "|";');
eval('echo call_spread(fn ($a) => $a + 1, 4), call_spread(fn () => 6), "|";');
eval('echo call_narrowed(fn () => 7), call_narrowed(8), "|";');
eval('echo call_callable("named_five"), call_callable([new Box(), "nine"]), call_callable("Box::ten"), "|";');
eval('function ev_fn() { return "e"; } echo call_callable("ev_fn");');
"#,
    );
    assert_eq!(out, "12|34|56|78|5910|e");
}

/// Named arguments to a method of an object the compiled site cannot type (an eval-declared
/// class, or any receiver typed only `object`) bind by name at run time instead of by position.
#[test]
fn test_named_args_bind_by_name_on_open_receivers() {
    let out = compile_and_run(
        r#"<?php
eval('class EvBuilder {
    public function withRouting(?Closure $using = null, array|string|null $web = null, ?string $commands = null, ?string $health = null, string $apiPrefix = "api") {
        return ($using === null ? "null" : "closure") . "|" . var_export($web, true) . "|" . var_export($commands, true) . "|" . var_export($health, true) . "|" . $apiPrefix;
    }
}');
class Local { public function withRouting($a = 1, $web = 2, $health = 3) { return "$a/$web/$health"; } }
function via_object(object $o) { return $o->withRouting(health: 'H', web: 'W'); }
$cls = 'EvBuilder';
$b = new $cls();
echo $b->withRouting(web: 'w.php', commands: 'c.php', health: '/up'), "\n";
echo $b->withRouting(null, 'x', health: '/h'), "\n";
echo via_object($b), "\n";
echo via_object(new Local()), "\n";
$m = $argc > 5 ? $b : new Local();
echo $m->withRouting(health: 'h2'), "\n";
"#,
    );
    assert_eq!(
        out,
        "null|'w.php'|'c.php'|'/up'|api\nnull|'x'|NULL|'/h'|api\nnull|'W'|NULL|'H'|api\n1/W/H\n1/2/h2\n"
    );
}

/// An eval call into an AOT instance or static method reports the frame the PHP source declares.
///
/// A method bridge is called DIRECTLY by eval, one argument per PHYSICAL parameter, so the
/// registration keeps the hidden slots the generated bridge takes while hiding them from PHP.
/// This fixture pins all four consequences of that split at once:
///
/// - An omitted trailing optional is not counted. `optionalTally(1)` must report one argument,
///   which is only possible if the hidden actual-argument count reaches the callee correctly.
/// - A surplus positional argument past the declared list still reaches `func_get_args()`, which
///   means it landed in the hidden collector rather than being dropped or rejected.
/// - A source-declared variadic tail still collects, and its hidden count parameter still
///   reports the real arity next to it.
/// - A named argument selects a PHP-visible parameter. Hidden slots register the EMPTY name and
///   a PHP parameter name is never empty, so none of them is reachable by name.
#[test]
fn test_eval_aot_method_frames_follow_the_declared_signature() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodFrameShape {
    public function optionalTally($first, $second = 5) {
        return func_num_args() . ":" . implode(",", func_get_args());
    }

    public static function staticTally($first, $second = 5) {
        return func_num_args() . ":" . implode(",", func_get_args());
    }

    public function variadicTally($first, $second = 5, ...$rest) {
        return func_num_args() . ":" . implode(",", func_get_args()) . ":" . implode(",", $rest);
    }
}

echo eval('$object = new EvalMethodFrameShape();
echo $object->optionalTally(1), "|", $object->optionalTally(1, 2), "|";
echo $object->optionalTally(1, 2, 3), "|";
echo EvalMethodFrameShape::staticTally(1), "|", EvalMethodFrameShape::staticTally(1, 2), "|";
echo $object->variadicTally(1), "|", $object->variadicTally(1, 2, 3, 4), "|";
return $object->optionalTally(first: 1);');
"#,
    );

    assert_eq!(out, "1:1|2:1,2|3:1,2,3|1:1|2:1,2|1:1:|4:1,2,3,4:3,4|1:1");
}

/// An unknown named argument reaches an AOT source variadic with its string key intact.
///
/// A free function is NOT called through its bridge directly: eval hands the descriptor invoker
/// one container holding exactly the supplied arguments, and the invoker synthesizes the hidden
/// slots itself. That container is indexed until an unknown name appears, at which point it
/// becomes associative. Ordering is the property under test: every positional entry keeps its own
/// integer key AHEAD of every string key, which is both what PHP's collected array looks like and
/// the only order the invoker's one-pass container validation accepts.
///
/// The second call also pins the named HOLE: the optional it skips still falls back to its
/// declared default rather than absorbing the named tail entry.
#[test]
fn test_eval_aot_source_variadic_keeps_unknown_named_argument_keys() {
    let out = compile_and_run(
        r#"<?php
function eval_named_tail_collect($first, ...$rest) {
    return $first . ":" . implode(",", array_keys($rest)) . ":" . implode(",", $rest);
}

function eval_named_tail_hole($first, $second = 5, ...$rest) {
    return $first . ":" . $second . ":" . implode(",", array_keys($rest));
}

echo eval('echo eval_named_tail_collect(1, 2, extra: 3), "|";
return eval_named_tail_hole(1, extra: 3);');
"#,
    );

    assert_eq!(out, "1:0,extra:2,3|1:5:extra");
}

/// Surplus positional arguments from eval reach an AOT function that declares no variadic.
///
/// PHP accepts them and `func_get_args()` still reports them, so the binder appends them to the
/// container past the declared parameters instead of rejecting the call; the invoker routes them
/// into the hidden collector. The omitted-optional call in the same fixture is the other half of
/// the contract: the container must NOT be padded out to the declared parameter count, or the
/// callee would report an argument the caller never passed.
#[test]
fn test_eval_aot_function_container_keeps_only_supplied_arguments() {
    let out = compile_and_run(
        r#"<?php
function eval_surplus_tally($first, $second = 5) {
    return func_num_args() . ":" . implode(",", func_get_args()) . ":" . $second;
}

echo eval('return eval_surplus_tally(1) . "|" . eval_surplus_tally(1, 2)
    . "|" . eval_surplus_tally(1, 2, 3, 4);');
"#,
    );

    assert_eq!(out, "1:1:5|2:1,2:2|4:1,2,3,4:2");
}

/// A by-reference variadic tail keeps per-element writeback while the container stays truncated.
///
/// This is the intersection of the two contracts that could plausibly have collided: the binder
/// now truncates the trailing optionals a caller omitted, and it also stages each positional tail
/// element as its own reference marker. An AOT caller lowers a by-reference variadic tail the
/// same way, so the collected array legitimately holds marker cells and the descriptor invoker's
/// verbatim tail copy is exactly what the callee expects.
///
/// The fixture also pins the `call_user_func()` degradation next to it: that path warns and binds
/// by value, so the caller's variable must be UNCHANGED while the callee still sees its value.
#[test]
fn test_eval_aot_by_reference_variadic_tail_writes_back_per_element() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_ref_tail_writer(&...$rest): string {
    $rest[0] = $rest[0] . "-written";
    return $rest[0];
}

echo eval('$value = "caller";
echo eval_ref_tail_writer($value), ":", $value, "|";
$second = "cuf";
echo call_user_func("eval_ref_tail_writer", $second), ":";
return $second;');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "caller-written:caller-written|cuf-written:cuf"
    );
    assert!(
        out.stderr.contains(
            "eval_ref_tail_writer(): Argument #1 ($rest) must be passed by reference, value given"
        ),
        "unexpected stderr: {}",
        out.stderr
    );
}

/// An eval call into an AOT method whose optional default has NO eval representation still sees
/// the optional as optional, and still reports the real argument count.
///
/// The eval default ABI can only carry a default whose VALUE it can represent. This one nests
/// twenty array levels deep, past `MAX_NATIVE_DEFAULT_CONSTANT_DEPTH`, so the bridge registers no
/// default for `$second` at all; an enum-case default is unrepresentable for the same reason. The
/// explicit signature shape the bridge registers is what keeps the call working: deriving arity
/// from the registered defaults would call `$second` MANDATORY here and reject `tally(1)`, and
/// would also decide the hidden collector carries no count, so `func_num_args()` would answer
/// the physical parameter count instead of the one the caller passed.
///
/// The AOT side is unaffected by that gap: the generated bridge materializes the default from the
/// expression directly. That asymmetry is exactly why the shape has to be positive metadata.
/// `count()` stands in for printing the values so the fixture needs no array-to-string coercion.
#[test]
fn test_eval_aot_method_optional_default_without_an_eval_representation_stays_optional() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodUnrepresentableDefault {
    public function tally($first, $second = [[[[[[[[[[[[[[[[[[[[1]]]]]]]]]]]]]]]]]]]]) {
        return func_num_args() . ":" . count(func_get_args()) . ":" . count($second);
    }

    public static function staticTally($first, $second = [[[[[[[[[[[[[[[[[[[[1]]]]]]]]]]]]]]]]]]]]) {
        return func_num_args() . ":" . count(func_get_args()) . ":" . count($second);
    }
}

echo eval('$object = new EvalMethodUnrepresentableDefault();
echo $object->tally(1), "|", $object->tally(1, [7, 8]), "|";
echo EvalMethodUnrepresentableDefault::staticTally(1), "|";
return $object->tally(first: 1);');
"#,
    );

    assert_eq!(out, "1:1:1|2:2:2|1:1:1|1:1:1");
}
