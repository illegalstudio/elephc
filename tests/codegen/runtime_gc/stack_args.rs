use crate::support::*;

#[test]
fn test_function_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
sum9(1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_instance_method_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class GreeterOverflow {
    public function greet($a, $b, $c, $d, $e, $f, string $message) {
        echo $message;
    }
}
$g = new GreeterOverflow();
$g->greet(1, 2, 3, 4, 5, 6, "hello");
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_constructor_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class ConstructorOverflow {
    public $message;
    public function __construct($a, $b, $c, $d, $e, $f, string $message) {
        $this->message = $message;
    }
}
$value = new ConstructorOverflow(1, 2, 3, 4, 5, 6, "stack");
echo $value->message;
"#,
    );
    assert_eq!(out, "stack");
}

#[test]
fn test_static_method_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
class StaticOverflow {
    public static function pick($a, $b, $c, $d, $e, $f, $g, $h) {
        echo $h;
    }
}
StaticOverflow::pick(1, 2, 3, 4, 5, 6, 7, 8);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_callable_variable_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
$fn = sum9(...);
$fn(1, 2, 3, 4, 5, 6, 7, 8, 9);
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_by_ref_argument_supports_large_stack_offsets() {
    let mut source = String::from("<?php\nfunction bump(&$value) { $value = $value + 1; }\nfunction large_frame() {\n");
    for i in 0..520 {
        source.push_str(&format!("    $slot{} = {};\n", i, i));
    }
    source.push_str("    bump($slot519);\n    echo $slot519;\n}\nlarge_frame();\n");

    let out = compile_and_run(&source);
    assert_eq!(out, "520");
}

#[test]
fn test_float_call_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9f(float $a, float $b, float $c, float $d, float $e, float $f, float $g, float $h, float $i) {
    echo (int) ($a + $b + $c + $d + $e + $f + $g + $h + $i);
}
sum9f(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
"#,
    );
    assert_eq!(out, "45");
}

#[test]
fn test_call_user_func_array_supports_stack_passed_overflow_args() {
    let out = compile_and_run(
        r#"<?php
function sum9($a, $b, $c, $d, $e, $f, $g, $h, $i) {
    echo $a + $b + $c + $d + $e + $f + $g + $h + $i;
}
call_user_func_array("sum9", [1, 2, 3, 4, 5, 6, 7, 8, 9]);
"#,
    );
    assert_eq!(out, "45");
}
