use super::*;

#[test]
fn test_json_exception_construct_and_get_message() {
    let out = compile_and_run(
        r#"<?php $e = new JsonException("decode failed"); echo $e->getMessage();"#,
    );
    assert_eq!(out, "decode failed");
}

#[test]
fn test_runtime_exception_construct_and_get_message() {
    let out = compile_and_run(
        r#"<?php $e = new RuntimeException("rte"); echo $e->getMessage();"#,
    );
    assert_eq!(out, "rte");
}

#[test]
fn test_json_exception_caught_as_json_exception() {
    let out = compile_and_run(
        r#"<?php
try { throw new JsonException("decode failed"); }
catch (JsonException $e) { echo "caught: " . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "caught: decode failed");
}

#[test]
fn test_json_exception_caught_as_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
try { throw new JsonException("again"); }
catch (RuntimeException $e) { echo "rte: " . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "rte: again");
}

#[test]
fn test_json_exception_caught_as_exception() {
    let out = compile_and_run(
        r#"<?php
try { throw new JsonException("third"); }
catch (Exception $e) { echo "ex: " . $e->getMessage(); }
"#,
    );
    assert_eq!(out, "ex: third");
}

#[test]
fn test_json_exception_instanceof_runtime_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new JsonException("x");
echo ($e instanceof RuntimeException ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_json_exception_instanceof_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new JsonException("x");
echo ($e instanceof Exception ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_json_exception_instanceof_throwable() {
    let out = compile_and_run(
        r#"<?php
$e = new JsonException("x");
echo ($e instanceof Throwable ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_runtime_exception_instanceof_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new RuntimeException("x");
echo ($e instanceof Exception ? "yes" : "no");
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_plain_exception_is_not_json_exception() {
    let out = compile_and_run(
        r#"<?php
$e = new Exception("plain");
echo ($e instanceof JsonException ? "yes" : "no");
"#,
    );
    assert_eq!(out, "no");
}

// JsonException::getCode() — the JSON_ERROR_* code that triggered the throw
// is exposed via Exception's standard $code property and getCode() accessor.

#[test]
fn test_json_exception_get_code_syntax() {
    let out = compile_and_run(
        r#"<?php
            try { json_decode("invalid", null, 512, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_json_exception_get_code_depth() {
    let out = compile_and_run(
        r#"<?php
            try { json_decode("[[1]]", false, 1, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_json_exception_get_code_utf16() {
    let out = compile_and_run(
        r#"<?php
            try { json_decode("\"\\uD83D\"", null, 512, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_json_exception_get_code_inf_or_nan() {
    let out = compile_and_run(
        r#"<?php
            try { json_encode(INF, JSON_THROW_ON_ERROR); echo "no throw"; }
            catch (JsonException $e) { echo $e->getCode(); }
        "#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_exception_get_code_user_constructor() {
    let out = compile_and_run(
        r#"<?php
            $e = new Exception("hi", 42);
            echo $e->getCode();
        "#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_exception_get_code_default_zero() {
    let out = compile_and_run(
        r#"<?php
            $e = new Exception("hi");
            echo $e->getCode();
        "#,
    );
    assert_eq!(out, "0");
}
