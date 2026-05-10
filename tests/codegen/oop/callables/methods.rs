//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP, callables methods, including first class callable instance method call user func with capture, first class callable inline instance method call user func with capture, and first class callable instance method call user func array with capture.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_first_class_callable_instance_method_call_user_func_with_capture() {
    let out = compile_and_run(
        r#"<?php
class MathBox {
    public function add_seven($n) {
        return $n + 7;
    }
}

$box = new MathBox();
$fn = $box->add_seven(...);
echo call_user_func($fn, 5);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_first_class_callable_inline_instance_method_call_user_func_with_capture() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public function greet($name) {
        return "Hi " . $name;
    }
}

$greeter = new Greeter();
echo call_user_func($greeter->greet(...), "Ada");
"#,
    );
    assert_eq!(out, "Hi Ada");
}

#[test]
fn test_first_class_callable_instance_method_call_user_func_array_with_capture() {
    let out = compile_and_run(
        r#"<?php
class MathBox {
    public function combine($a, $b) {
        return $a * 10 + $b;
    }
}

$box = new MathBox();
$fn = $box->combine(...);
echo call_user_func_array($fn, [3, 4]);
"#,
    );
    assert_eq!(out, "34");
}

#[test]
fn test_call_user_func_first_class_callable_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
function bump(&$n) {
    $n = $n + 1;
}

$f = bump(...);
$value = 5;
call_user_func($f, $value);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_call_user_func_closure_alias_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
$f = function (&$x) {
    $x = $x + 1;
};
$g = $f;
$value = 5;
call_user_func($g, $value);
echo $value;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_instance_method_preserves_multiple_byref_array_params() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public function bar(array &$a, array &$b): void {
        $a[0] = 1;
        $b[0] = 2;
    }
}

$x = [0];
$y = [0];
$foo = new Foo();
$foo->bar($x, $y);
echo $x[0];
echo $y[0];
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_first_class_callable_instance_method_indirect_call() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public function greet($name) {
        return "Hello " . $name;
    }
}

$greeter = new Greeter();
$fn = $greeter->greet(...);
echo $fn("Ada");
"#,
    );
    assert_eq!(out, "Hello Ada");
}

#[test]
fn test_first_class_callable_instance_method_array_map_with_capture() {
    let out = compile_and_run(
        r#"<?php
class MathBox {
    public function triple($n) {
        return $n * 3;
    }
}

$box = new MathBox();
$fn = $box->triple(...);
$values = array_map($fn, [1, 2, 3]);
echo $values[0];
echo ":";
echo $values[2];
"#,
    );
    assert_eq!(out, "3:9");
}

#[test]
fn test_first_class_callable_inline_instance_method_array_map_with_capture() {
    let out = compile_and_run(
        r#"<?php
class MathBox {
    public function double($n) {
        return $n * 2;
    }
}

$box = new MathBox();
$values = array_map($box->double(...), [2, 4]);
echo $values[0];
echo ":";
echo $values[1];
"#,
    );
    assert_eq!(out, "4:8");
}

#[test]
fn test_first_class_callable_inline_instance_method_array_map_string_return_with_capture() {
    let out = compile_and_run(
        r#"<?php
class Formatter {
    public function bracket(string $value): string {
        return "[" . $value . "]";
    }
}

$formatter = new Formatter();
$values = array_map($formatter->bracket(...), ["a", "b"]);
echo $values[0];
echo ":";
echo $values[1];
"#,
    );
    assert_eq!(out, "[a]:[b]");
}

#[test]
fn test_first_class_callable_inline_instance_method_array_filter_with_capture() {
    let out = compile_and_run(
        r#"<?php
class FilterBox {
    public function keep($n) {
        return $n > 2;
    }
}

$box = new FilterBox();
$values = array_filter([1, 3, 4], $box->keep(...));
echo count($values);
foreach ($values as $value) {
    echo ":";
    echo $value;
}
"#,
    );
    assert_eq!(out, "2:3:4");
}

#[test]
fn test_first_class_callable_instance_method_preserves_by_ref_params() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public function bump(&$n) {
        $n = $n + 2;
    }
}

$counter = new Counter();
$fn = $counter->bump(...);
$value = 5;
$fn($value);
echo $value;
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_first_class_callable_instance_method_receiver_name_can_match_param() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function add($box) {
        return $box + 1;
    }
}

$box = new Box();
$fn = $box->add(...);
echo $fn(10);
"#,
    );
    assert_eq!(out, "11");
}

#[test]
fn test_first_class_callable_static_late_bound_method_indirect_call() {
    let out = compile_and_run(
        r#"<?php
class BaseMaker {
    public static function run() {
        $fn = static::label(...);
        return $fn();
    }

    public static function label() {
        return "base";
    }
}

class ChildMaker extends BaseMaker {
    public static function label() {
        return "child";
    }
}

echo BaseMaker::run();
echo ":";
echo ChildMaker::run();
"#,
    );
    assert_eq!(out, "base:child");
}

#[test]
fn test_first_class_callable_static_late_bound_from_instance_method() {
    let out = compile_and_run(
        r#"<?php
class BaseInstanceMaker {
    public function run() {
        $fn = static::label(...);
        return $fn();
    }

    public static function label() {
        return "base";
    }
}

class ChildInstanceMaker extends BaseInstanceMaker {
    public static function label() {
        return "child";
    }
}

$base = new BaseInstanceMaker();
$child = new ChildInstanceMaker();
echo $base->run();
echo ":";
echo $child->run();
"#,
    );
    assert_eq!(out, "base:child");
}

#[test]
fn test_first_class_callable_static_late_bound_array_map_with_capture() {
    let out = compile_and_run(
        r#"<?php
class BaseMapper {
    public static function run() {
        $values = array_map(static::offset(...), [1, 2]);
        echo $values[0];
        echo ":";
        echo $values[1];
    }

    public static function offset($n) {
        return $n + 10;
    }
}

class ChildMapper extends BaseMapper {
    public static function offset($n) {
        return $n + 20;
    }
}

BaseMapper::run();
echo "|";
ChildMapper::run();
"#,
    );
    assert_eq!(out, "11:12|21:22");
}
