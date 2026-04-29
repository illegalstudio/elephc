use super::*;

#[test]
fn test_gettype_int() {
    let out = compile_and_run("<?php echo gettype(42);");
    assert_eq!(out, "integer");
}

#[test]
fn test_gettype_float() {
    let out = compile_and_run("<?php echo gettype(3.14);");
    assert_eq!(out, "double");
}

#[test]
fn test_gettype_string() {
    let out = compile_and_run("<?php echo gettype(\"hi\");");
    assert_eq!(out, "string");
}

#[test]
fn test_gettype_bool() {
    let out = compile_and_run("<?php echo gettype(true);");
    assert_eq!(out, "boolean");
}

#[test]
fn test_gettype_null() {
    let out = compile_and_run("<?php echo gettype(null);");
    assert_eq!(out, "NULL");
}

#[test]
fn test_gettype_mixed_returns_concrete_payload_type() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "i" => 42,
    "s" => "hi",
    "n" => null,
    "a" => [1, 2],
    "b" => true,
];
echo gettype($map["i"]);
echo "|";
echo gettype($map["s"]);
echo "|";
echo gettype($map["n"]);
echo "|";
echo gettype($map["a"]);
echo "|";
echo gettype($map["b"]);
"#,
    );
    assert_eq!(out, "integer|string|NULL|array|boolean");
}

// --- empty ---

#[test]
fn test_empty_zero() {
    let out = compile_and_run("<?php echo empty(0);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_nonzero() {
    let out = compile_and_run("<?php echo empty(42);");
    assert_eq!(out, "");
}

#[test]
fn test_empty_empty_string() {
    let out = compile_and_run("<?php echo empty(\"\");");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_nonempty_string() {
    let out = compile_and_run("<?php echo empty(\"hi\");");
    assert_eq!(out, "");
}

#[test]
fn test_empty_null() {
    let out = compile_and_run("<?php echo empty(null);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_false() {
    let out = compile_and_run("<?php echo empty(false);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_true() {
    let out = compile_and_run("<?php echo empty(true);");
    assert_eq!(out, "");
}

#[test]
fn test_empty_mixed_uses_boxed_payload_semantics() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "zero" => 0,
    "blank" => "",
    "null" => null,
    "arr" => [],
    "one" => 1,
    "text" => "hi",
];
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["blank"]) ? "1" : "0";
echo empty($map["null"]) ? "1" : "0";
echo empty($map["arr"]) ? "1" : "0";
echo empty($map["one"]) ? "1" : "0";
echo empty($map["text"]) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111100");
}

// --- unset ---

#[test]
fn test_unset_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
unset($x);
echo is_null($x);
"#,
    );
    assert_eq!(out, "1");
}

// --- settype ---

#[test]
fn test_settype_to_string() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
settype($x, "string");
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_settype_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = 3.7;
settype($x, "integer");
echo $x;
"#,
    );
    assert_eq!(out, "3");
}

// --- Missing type function tests ---
