//! Purpose:
//! Integration or regression tests for object-oriented callable codegen, including captured method
//! and static first-class callables used through direct calls, indirect calls, and callback builtins.
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
fn test_array_filter_evaluates_array_before_method_callable_receiver() {
    let out = compile_and_run(
        r#"<?php
function values() {
    echo "array:";
    return [1, 3];
}

class FilterBox {
    public function __construct() {
        echo "receiver:";
    }

    public function keep($n) {
        return $n > 1;
    }
}

$values = array_filter(values(), (new FilterBox())->keep(...));
echo count($values);
"#,
    );
    assert_eq!(out, "array:receiver:1");
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

#[test]
fn test_first_class_callable_instance_method_array_reduce_with_capture() {
    let out = compile_and_run(
        r#"<?php
class Reducer {
    public function add_offset($carry, $item) {
        return $carry + $item + 10;
    }
}

$reducer = new Reducer();
$fn = $reducer->add_offset(...);
echo array_reduce([1, 2], $fn, 0);
"#,
    );
    assert_eq!(out, "23");
}

#[test]
fn test_array_reduce_evaluates_args_left_to_right_for_method_callable() {
    let out = compile_and_run(
        r#"<?php
function values() {
    echo "array:";
    return [1, 2];
}

function initial() {
    echo "initial:";
    return 0;
}

class Reducer {
    public function __construct() {
        echo "receiver:";
    }

    public function add($carry, $item) {
        return $carry + $item;
    }
}

echo array_reduce(values(), (new Reducer())->add(...), initial());
"#,
    );
    assert_eq!(out, "array:receiver:initial:3");
}

#[test]
fn test_array_filter_accepts_complex_noncaptured_callable_expression() {
    let out = compile_and_run(
        r#"<?php
function keep_even($n) {
    return $n % 2 == 0;
}

function keep_big($n) {
    return $n > 2;
}

$use_even = true;
$values = array_filter([1, 2, 3, 4], $use_even ? keep_even(...) : keep_big(...));
echo count($values);
foreach ($values as $value) {
    echo ":";
    echo $value;
}
"#,
    );
    assert_eq!(out, "2:2:4");
}

#[test]
fn test_first_class_callable_instance_method_array_walk_with_capture() {
    let out = compile_and_run(
        r#"<?php
class Walker {
    public function show($item) {
        echo $item + 5;
        echo ":";
    }
}

$walker = new Walker();
array_walk([1, 2], $walker->show(...));
"#,
    );
    assert_eq!(out, "6:7:");
}

#[test]
fn test_first_class_callable_instance_method_usort_with_capture() {
    let out = compile_and_run(
        r#"<?php
class Sorter {
    public function desc($a, $b) {
        return $b - $a;
    }
}

$sorter = new Sorter();
$values = [1, 3, 2];
usort($values, $sorter->desc(...));
foreach ($values as $value) {
    echo $value;
}
"#,
    );
    assert_eq!(out, "321");
}

#[test]
fn test_first_class_callable_instance_method_uksort_with_capture() {
    let out = compile_and_run(
        r#"<?php
class KeySorter {
    public function desc($a, $b) {
        return $b - $a;
    }
}

$sorter = new KeySorter();
$values = [1, 3, 2];
uksort($values, $sorter->desc(...));
foreach ($values as $value) {
    echo $value;
}
"#,
    );
    assert_eq!(out, "321");
}

#[test]
fn test_first_class_callable_instance_method_uasort_with_capture() {
    let out = compile_and_run(
        r#"<?php
class ValueSorter {
    public function asc($a, $b) {
        return $a - $b;
    }
}

$sorter = new ValueSorter();
$values = [3, 1, 2];
uasort($values, $sorter->asc(...));
foreach ($values as $value) {
    echo $value;
}
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_first_class_callable_static_late_bound_array_reduce_with_capture() {
    let out = compile_and_run(
        r#"<?php
class BaseReducer {
    public static function run() {
        return array_reduce([1, 2], static::add(...), 0);
    }

    public static function add($carry, $item) {
        return $carry + $item + 10;
    }
}

class ChildReducer extends BaseReducer {
    public static function add($carry, $item) {
        return $carry + $item + 20;
    }
}

echo BaseReducer::run();
echo ":";
echo ChildReducer::run();
"#,
    );
    assert_eq!(out, "23:43");
}

#[test]
fn test_first_class_callable_static_late_bound_remaining_callback_runtimes_with_capture() {
    let out = compile_and_run(
        r#"<?php
class BaseCallbacks {
    public static function run() {
        echo array_reduce([1, 2], static::add(...), 0);
        echo ":";
        array_walk([1, 2], static::show(...));
        echo ":";

        $usorted = [1, 3, 2];
        usort($usorted, static::compare(...));
        foreach ($usorted as $value) {
            echo $value;
        }
        echo ":";

        $uksorted = [1, 3, 2];
        uksort($uksorted, static::compare(...));
        foreach ($uksorted as $value) {
            echo $value;
        }
        echo ":";

        $uasorted = [1, 3, 2];
        uasort($uasorted, static::compare(...));
        foreach ($uasorted as $value) {
            echo $value;
        }
    }

    public static function add($carry, $item) {
        return $carry + $item + 10;
    }

    public static function show($item) {
        echo $item + 10;
        echo ",";
    }

    public static function compare($a, $b) {
        return $b - $a;
    }
}

class ChildCallbacks extends BaseCallbacks {
    public static function add($carry, $item) {
        return $carry + $item + 20;
    }

    public static function show($item) {
        echo $item + 20;
        echo ",";
    }

    public static function compare($a, $b) {
        return $a - $b;
    }
}

BaseCallbacks::run();
echo "|";
ChildCallbacks::run();
"#,
    );
    assert_eq!(out, "23:11,12,:321:321:321|43:21,22,:123:123:123");
}

#[test]
fn test_direct_first_class_callable_instance_method_expr_call() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public function greet($name) {
        return "Hi " . $name;
    }
}

$greeter = new Greeter();
echo ($greeter->greet(...))("Ada");
"#,
    );
    assert_eq!(out, "Hi Ada");
}

#[test]
fn test_direct_first_class_callable_expr_call_evaluates_receiver_before_args() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public function __construct() {
        echo "receiver:";
    }

    public function greet($name) {
        return $name;
    }
}

function name_arg() {
    echo "arg:";
    return "Ada";
}

echo ((new Greeter())->greet(...))(name_arg());
"#,
    );
    assert_eq!(out, "receiver:arg:Ada");
}

#[test]
fn test_parenthesized_captured_first_class_callable_variable_expr_call() {
    let out = compile_and_run(
        r#"<?php
class Bumper {
    public function apply($n) {
        return $n + 7;
    }
}

$bumper = new Bumper();
$fn = $bumper->apply(...);
echo ($fn)(5);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_first_class_callable_non_local_method_receiver() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public function __construct(private string $prefix) {}

    public function greet(string $name): string {
        return $this->prefix . $name;
    }
}

$fn = (new Greeter("Hi "))->greet(...);
echo $fn("Ada");
"#,
    );
    assert_eq!(out, "Hi Ada");
}
