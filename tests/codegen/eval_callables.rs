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
