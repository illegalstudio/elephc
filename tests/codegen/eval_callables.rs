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

use crate::support::{compile_and_run, compile_and_run_capture};

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
    let out = compile_and_run(
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
