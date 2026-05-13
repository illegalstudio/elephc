use crate::support::*;

#[test]
fn test_json_decode_default_returns_stdclass() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}"));"#);
    assert_eq!(out, "object");
}

#[test]
fn test_json_decode_explicit_false_returns_stdclass() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", false));"#);
    assert_eq!(out, "object");
}

#[test]
fn test_json_decode_explicit_null_returns_stdclass() {
    // PHP semantics: $associative=null is equivalent to false → stdClass.
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", null));"#);
    assert_eq!(out, "object");
}

#[test]
fn test_json_decode_assoc_true_returns_array() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", true));"#);
    assert_eq!(out, "array");
}

#[test]
fn test_json_decode_stdclass_property_read() {
    let out = compile_and_run(
        r#"<?php $o = json_decode("{\"name\":\"Alice\"}"); echo $o->name;"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_json_decode_stdclass_int_property() {
    let out = compile_and_run(
        r#"<?php $o = json_decode("{\"age\":30}"); echo $o->age;"#,
    );
    assert_eq!(out, "30");
}

#[test]
fn test_json_decode_stdclass_missing_property_is_null() {
    let out = compile_and_run(
        r#"<?php $o = json_decode("{\"a\":1}"); echo gettype($o->missing);"#,
    );
    assert_eq!(out, "NULL");
}

#[test]
fn test_json_decode_stdclass_round_trip() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\":1,\"b\":2}"));"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

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

#[test]
fn test_new_stdclass_is_empty_object() {
    // A freshly-`new`'d stdClass round-trips through json_encode as {}.
    let out = compile_and_run(
        r#"<?php $o = new stdClass(); echo json_encode($o);"#,
    );
    assert_eq!(out, "{}");
}

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

#[test]
fn test_json_decode_stdclass_with_null_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\":null}"));"#,
    );
    assert_eq!(out, r#"{"a":null}"#);
}

#[test]
fn test_json_decode_stdclass_with_bool_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\":true}"));"#,
    );
    assert_eq!(out, r#"{"a":true}"#);
}
