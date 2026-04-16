use crate::support::*;

#[test]
fn test_expr_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$greet = function($name) { return "Hello " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_expr_call_returns_float() {
    let out = compile_and_run(
        r#"<?php
$calc = function($x) { return $x * 3.14; };
echo $calc(2.0);
"#,
    );
    assert_eq!(out, "6.28");
}

#[test]
fn test_expr_call_returns_int() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(21);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_expr_call_string_in_concat() {
    let out = compile_and_run(
        r#"<?php
$tag = function($s) { return "<b>" . $s . "</b>"; };
echo "Result: " . $tag("hello");
"#,
    );
    assert_eq!(out, "Result: <b>hello</b>");
}

#[test]
fn test_closure_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$fn = function() { return "test"; };
$result = $fn();
echo $result;
"#,
    );
    assert_eq!(out, "test");
}
