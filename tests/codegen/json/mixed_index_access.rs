//! Purpose:
//! Provides Mixed JSON access tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Decoded arrays and objects must support direct index/property access on Mixed cells.

use crate::support::*;

/// String-keyed access on a Mixed assoc receiver (the common
/// `json_decode($json, true)["k"]` shape).
#[test]
fn test_mixed_string_index_on_assoc() {
    let out = compile_and_run(
        r#"<?php
            $x = json_decode("{\"name\":\"Alice\",\"age\":30}", true);
            echo $x["name"] . "/" . $x["age"];
        "#,
    );
    assert_eq!(out, "Alice/30");
}

/// Regression for issue #179: assoc-mode json_decode must apply PHP array-key
/// coercion to JSON object keys, so integer-form strings and integer access hit
/// the same entry while non-integer strings such as leading-zero keys stay
/// string-keyed.
#[test]
fn test_mixed_assoc_numeric_json_object_keys_coerce_like_php() {
    let out = compile_and_run(
        r#"<?php
            $json = "{\"1\":\"one\",\"2\":\"two\",\"01\":\"leading\",\"-1\":\"neg\",\"name\":\"test\"}";
            $a = json_decode($json, true);
            echo $a["1"] . "/" . $a[1] . "/" . $a["01"] . "/" . $a["-1"] . "/" . $a[-1] . "/" . $a["name"];
        "#,
    );
    assert_eq!(out, "one/one/leading/neg/neg/test");
}

/// Integer-keyed access on a Mixed indexed array.
#[test]
fn test_mixed_int_index_on_indexed() {
    let out = compile_and_run(
        r#"<?php
            $x = json_decode("[10, 20, 30]", true);
            echo $x[0] . "/" . $x[1] . "/" . $x[2];
        "#,
    );
    assert_eq!(out, "10/20/30");
}

/// Chained `[k]` access traversing through nested arrays inside Mixed.
#[test]
fn test_mixed_chained_access() {
    let out = compile_and_run(
        r#"<?php
            $json = "{\"users\":[{\"name\":\"Alice\"},{\"name\":\"Bob\"}]}";
            $data = json_decode($json, true);
            echo $data["users"][0]["name"] . "," . $data["users"][1]["name"];
        "#,
    );
    assert_eq!(out, "Alice,Bob");
}

/// Chained index access through nested assoc arrays with in-place mutation via json_decode.
#[test]
fn test_mixed_chained_assoc_array_assignment() {
    let out = compile_and_run(
        r#"<?php
            $data = json_decode("{\"a\":[{\"b\":\"old\"}]}", true);
            $data["a"][0]["b"] = "changed";
            echo json_encode($data);
        "#,
    );
    assert_eq!(out, "{\"a\":[{\"b\":\"changed\"}]}");
}

/// stdClass receiver via array bracket access — PHP allows it for objects
/// with public properties accessed by string key (e.g. ArrayAccess interface
/// is the strict path; for stdClass elefant emulates the friendly idiom).
#[test]
fn test_mixed_string_index_on_stdclass() {
    let out = compile_and_run(
        r#"<?php
            $obj = json_decode("{\"name\":\"Bob\"}");
            echo $obj["name"];
        "#,
    );
    assert_eq!(out, "Bob");
}

/// Missing keys decode to Mixed(null) instead of erroring out — matches
/// PHP's quiet "undefined index" warning behavior collapsed to a typed null.
#[test]
fn test_mixed_index_missing_key_is_null() {
    let out = compile_and_run(
        r#"<?php
            $x = json_decode("{}", true);
            echo gettype($x["missing"]);
        "#,
    );
    assert_eq!(out, "NULL");
}

/// Out-of-bounds indexed access also returns Mixed(null).
#[test]
fn test_mixed_index_out_of_bounds_is_null() {
    let out = compile_and_run(
        r#"<?php
            $x = json_decode("[1, 2, 3]", true);
            echo gettype($x[5]);
        "#,
    );
    assert_eq!(out, "NULL");
}

/// Negative indexed access also returns Mixed(null) (no PHP wrap-around).
#[test]
fn test_mixed_index_negative_is_null() {
    let out = compile_and_run(
        r#"<?php
            $x = json_decode("[1, 2, 3]", true);
            echo gettype($x[-1]);
        "#,
    );
    assert_eq!(out, "NULL");
}

/// `count()` on a Mixed indexed array reads from the array header.
#[test]
fn test_mixed_count_indexed() {
    let out = compile_and_run(
        r#"<?php echo count(json_decode("[1,2,3,4,5]", true));"#,
    );
    assert_eq!(out, "5");
}

/// `count()` on a Mixed assoc array reads from the hash header.
#[test]
fn test_mixed_count_assoc() {
    let out = compile_and_run(
        r#"<?php echo count(json_decode("{\"a\":1,\"b\":2,\"c\":3}", true));"#,
    );
    assert_eq!(out, "3");
}

/// `count()` on a non-container Mixed payload returns 0 (PHP would emit a
/// warning and return 1 in older versions / 0 in PHP 8+; elefant collapses
/// to 0).
#[test]
fn test_mixed_count_scalar_is_zero() {
    let out = compile_and_run(r#"<?php echo count(json_decode("42"));"#);
    assert_eq!(out, "0");
}

/// Nested access with int key first, then string key: `arr[0]["x"]` on an
/// array of assoc objects returned by json_decode.
#[test]
fn test_mixed_index_nested_int_then_string() {
    let out = compile_and_run(
        r#"<?php
            $arr = json_decode("[{\"x\":1},{\"x\":2},{\"x\":3}]", true);
            echo $arr[0]["x"] . $arr[1]["x"] . $arr[2]["x"];
        "#,
    );
    assert_eq!(out, "123");
}

/// json_decode default returns stdClass for the outer; `[]` on the
/// stdClass property still works via the dispatch helper.
#[test]
fn test_mixed_index_through_default_stdclass() {
    let out = compile_and_run(
        r#"<?php
            $o = json_decode("{\"items\":[10,20]}");
            echo gettype($o->items) . ":" . $o->items[0] . "," . $o->items[1];
        "#,
    );
    assert_eq!(out, "array:10,20");
}
