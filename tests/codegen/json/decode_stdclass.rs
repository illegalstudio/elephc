//! Purpose:
//! Provides json_decode stdClass behavior tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Default object decoding must expose dynamic properties and assoc=true must keep arrays.

use crate::support::*;

/// Verifies json_decode("{}") returns an object type (stdClass) by default.
#[test]
fn test_json_decode_default_returns_stdclass() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}"));"#);
    assert_eq!(out, "object");
}

/// Verifies json_decode("{}", false) explicitly returns an object type (stdClass).
#[test]
fn test_json_decode_explicit_false_returns_stdclass() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", false));"#);
    assert_eq!(out, "object");
}

/// Verifies json_decode("{}", null) returns object type (PHP: null ≡ false → stdClass).
#[test]
fn test_json_decode_explicit_null_returns_stdclass() {
    // PHP semantics: $associative=null is equivalent to false → stdClass.
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", null));"#);
    assert_eq!(out, "object");
}

/// Verifies json_decode("{}", true) returns an array type.
#[test]
fn test_json_decode_assoc_true_returns_array() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", true));"#);
    assert_eq!(out, "array");
}

/// Verifies a stdClass property read from json_decode returns the expected string value.
#[test]
fn test_json_decode_stdclass_property_read() {
    let out = compile_and_run(
        r#"<?php $o = json_decode("{\"name\":\"Alice\"}"); echo $o->name;"#,
    );
    assert_eq!(out, "Alice");
}

/// Verifies a stdClass property holding an integer is readable and returns the integer value.
#[test]
fn test_json_decode_stdclass_int_property() {
    let out = compile_and_run(
        r#"<?php $o = json_decode("{\"age\":30}"); echo $o->age;"#,
    );
    assert_eq!(out, "30");
}

/// Verifies reading a non-existent property on stdClass returns NULL (not an error).
#[test]
fn test_json_decode_stdclass_missing_property_is_null() {
    let out = compile_and_run(
        r#"<?php $o = json_decode("{\"a\":1}"); echo gettype($o->missing);"#,
    );
    assert_eq!(out, "NULL");
}

/// Verifies stdClass decoded from JSON round-trips through json_encode back to JSON.
#[test]
fn test_json_decode_stdclass_round_trip() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\":1,\"b\":2}"));"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

/// Verifies chained property access ($o->outer->inner) works on nested stdClass.
#[test]
fn test_json_decode_nested_stdclass() {
    let out = compile_and_run(
        r#"<?php
            $payload = "{\"outer\": {\"inner\": \"value\"}}";
            $o = json_decode($payload);
            echo $o->outer->inner;
        "#,
    );
    assert_eq!(out, "value");
}

/// Verifies a freshly new'd stdClass round-trips through json_encode as {}.
#[test]
fn test_new_stdclass_is_empty_object() {
    // A freshly-`new`'d stdClass round-trips through json_encode as {}.
    let out = compile_and_run(
        r#"<?php $o = new stdClass(); echo json_encode($o);"#,
    );
    assert_eq!(out, "{}");
}

/// Verifies dynamic property writes on a new stdClass are observable via reads.
#[test]
fn test_new_stdclass_dynamic_property_writes() {
    let out = compile_and_run(
        r#"<?php
            $o = new stdClass();
            $o->name = "Alice";
            $o->age = 30;
            echo $o->name . " " . $o->age;
        "#,
    );
    assert_eq!(out, "Alice 30");
}

/// Verifies overwriting an existing property keeps the latest value.
#[test]
fn test_new_stdclass_overwrite_property() {
    let out = compile_and_run(
        r#"<?php
            $o = new stdClass();
            $o->x = 1;
            $o->x = 99;
            echo $o->x;
        "#,
    );
    assert_eq!(out, "99");
}

/// Verifies a stdClass with mixed-type properties round-trips through json_encode.
#[test]
fn test_new_stdclass_round_trip_through_json() {
    let out = compile_and_run(
        r#"<?php
            $o = new stdClass();
            $o->a = 1;
            $o->b = "two";
            $o->c = true;
            echo json_encode($o);
        "#,
    );
    // Property iteration order is the hash insertion order.
    assert_eq!(out, r#"{"a":1,"b":"two","c":true}"#);
}

/// Verifies a new stdClass instance is an instanceof stdClass.
#[test]
fn test_stdclass_instanceof_stdclass() {
    let out = compile_and_run(
        r#"<?php
            $o = new stdClass();
            echo ($o instanceof stdClass) ? "yes" : "no";
        "#,
    );
    assert_eq!(out, "yes");
}

/// Verifies a stdClass decoded from JSON is an instanceof stdClass.
#[test]
fn test_json_decode_stdclass_passes_instanceof() {
    let out = compile_and_run(
        r#"<?php
            $o = json_decode("{\"x\":1}");
            echo ($o instanceof stdClass) ? "yes" : "no";
        "#,
    );
    assert_eq!(out, "yes");
}

/// Verifies stdClass property access returns Mixed; gettype reports the underlying runtime type
/// (integer for ints, string for strings).
#[test]
fn test_json_decode_stdclass_property_is_mixed() {
    // Reading a property on stdClass returns Mixed, so gettype reflects
    // the underlying runtime type — ints stay "integer", strings stay "string".
    let out = compile_and_run(
        r#"<?php
            $o = json_decode("{\"n\":42,\"s\":\"hi\"}");
            echo gettype($o->n) . "," . gettype($o->s);
        "#,
    );
    assert_eq!(out, "integer,string");
}

/// Verifies a stdClass property decoded as an array is observable as array type.
#[test]
fn test_json_decode_stdclass_array_property() {
    let out = compile_and_run(
        r#"<?php
            $o = json_decode("{\"items\":[1,2,3]}");
            echo gettype($o->items);
        "#,
    );
    assert_eq!(out, "array");
}

/// Verifies chained access through an array inside an object ($obj->users[0]->name) works,
/// and that mutating a nested array element is reflected in the re-encoded JSON.
#[test]
fn test_json_decode_nested_stdclass_array_assignment() {
    let out = compile_and_run(
        r#"<?php
            $obj = json_decode("{\"users\":[{\"name\":\"Ada\",\"tags\":[\"x\",\"y\"]}]}");
            echo $obj->users[0]->name;
            echo "\n";
            $obj->users[0]->tags[1] = "changed";
            echo json_encode($obj);
            echo "\n";
        "#,
    );
    assert_eq!(
        out,
        "Ada\n{\"users\":[{\"name\":\"Ada\",\"tags\":[\"x\",\"changed\"]}]}\n"
    );
}

/// Verifies an array of JSON objects is decoded as an array (not a single stdClass).
#[test]
fn test_json_decode_stdclass_in_array() {
    let out = compile_and_run(
        r#"<?php
            $o = json_decode("[{\"x\":1},{\"x\":2}]");
            echo gettype($o);
        "#,
    );
    assert_eq!(out, "array");
}

/// Verifies an object with a null value round-trips through encode/decode correctly.
#[test]
fn test_json_decode_stdclass_with_null_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\":null}"));"#,
    );
    assert_eq!(out, r#"{"a":null}"#);
}

/// Verifies an object with a boolean value round-trips through encode/decode correctly.
#[test]
fn test_json_decode_stdclass_with_bool_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\":true}"));"#,
    );
    assert_eq!(out, r#"{"a":true}"#);
}
