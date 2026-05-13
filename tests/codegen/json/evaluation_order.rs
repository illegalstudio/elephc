use super::*;

#[test]
fn test_json_encode_evaluates_value_before_flags_and_depth() {
    let out = compile_and_run(
        r#"<?php
function value_arg() { echo "V"; return "x"; }
function flags_arg() { echo "F"; return 0; }
function depth_arg() { echo "D"; return 512; }
echo json_encode(value_arg(), flags_arg(), depth_arg());
"#,
    );
    assert_eq!(out, "VFD\"x\"");
}

#[test]
fn test_json_decode_evaluates_arguments_left_to_right() {
    let out = compile_and_run(
        r#"<?php
function json_arg() { echo "J"; return "{\"a\":1}"; }
function assoc_arg() { echo "A"; return true; }
function depth_arg() { echo "D"; return 512; }
function flags_arg() { echo "F"; return 0; }
echo gettype(json_decode(json_arg(), assoc_arg(), depth_arg(), flags_arg()));
"#,
    );
    assert_eq!(out, "JADFarray");
}

#[test]
fn test_json_validate_evaluates_arguments_left_to_right() {
    let out = compile_and_run(
        r#"<?php
function json_arg() { echo "J"; return "[1]"; }
function depth_arg() { echo "D"; return 512; }
function flags_arg() { echo "F"; return 0; }
echo json_validate(json_arg(), depth_arg(), flags_arg()) ? "ok" : "no";
"#,
    );
    assert_eq!(out, "JDFok");
}

#[test]
fn test_json_decode_string_associative_uses_php_truthiness() {
    let out = compile_and_run(
        r#"<?php
echo gettype(json_decode("{}", "")) . "\n";
echo gettype(json_decode("{}", "0")) . "\n";
echo gettype(json_decode("{}", "1"));
"#,
    );
    assert_eq!(out, "object\nobject\narray");
}

#[test]
fn test_json_decode_object_as_array_flag_applies_when_associative_is_null() {
    let out = compile_and_run(
        r#"<?php
echo gettype(json_decode("{}", null, 512, JSON_OBJECT_AS_ARRAY)) . "\n";
echo gettype(json_decode("{}", false, 512, JSON_OBJECT_AS_ARRAY));
"#,
    );
    assert_eq!(out, "array\nobject");
}

#[test]
fn test_json_string_arguments_accept_scalar_coercion() {
    let out = compile_and_run(
        r#"<?php
echo json_validate(123) ? "valid" : "invalid";
echo ":";
echo json_decode(123);
"#,
    );
    assert_eq!(out, "valid:123");
}
