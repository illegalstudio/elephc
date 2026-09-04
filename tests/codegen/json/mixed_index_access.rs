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

/// Verifies a decoded `stdClass` REFUSES bracket access, the way PHP does.
///
/// This test used to assert `Bob`, on the stated belief that "PHP allows it for objects with
/// public properties accessed by string key" and that bracket access on `stdClass` was a
/// friendly idiom worth emulating. Measured against 8.5, that belief is wrong: the very program
/// below stops with `Cannot use object of type stdClass as array`. The old expectation was read
/// off the implementation, so it pinned the divergence in place instead of catching it.
///
/// `json_decode` without `true` is exactly where a program meets this, which is why the case
/// lives here rather than with the other object tests.
#[test]
fn test_mixed_string_index_on_stdclass_is_refused() {
    let err = compile_and_run_expect_failure(
        r#"<?php
            $obj = json_decode("{\"name\":\"Bob\"}");
            echo $obj["name"];
        "#,
    );
    assert!(
        err.contains("Fatal error: Uncaught Error: Cannot use object of type stdClass as array"),
        "{err}"
    );
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

/// `count()` on a non-container Mixed payload raises PHP 8's TypeError.
///
/// This asserted `0` and passed — which is what a divergence looks like once a test
/// records it. Reference PHP has thrown here since 8.0; the quiet answer dates from the
/// 7.2 warning and was never revisited. The message is php-src's own, and it names a
/// boolean by its VALUE (`false given`, never `bool given`), so the arm is per-tag.
#[test]
fn test_mixed_count_scalar_throws_php_type_error() {
    let out = compile_and_run(
        r#"<?php
        try {
            echo count(json_decode("42"));
        } catch (TypeError $e) {
            echo $e->getMessage();
        }
        "#,
    );
    assert_eq!(
        out,
        "count(): Argument #1 ($value) must be of type Countable|array, int given"
    );
}

/// `count()` names every type PHP names, with PHP's own spelling.
///
/// All eight wordings were read off `php -n` 8.5.6 rather than derived from the tag names, which
/// is what catches the boolean pair: PHP spells the VALUE, so `true` and `false` are different
/// words where a type-driven table would have produced `bool` twice. The two containers at the
/// end are the control — a fix that raised for everything would look just as green without them.
#[test]
fn test_count_names_every_rejected_type_like_php() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("/dev/null", "r");
$vals = [1, "s", 1.5, true, false, null, $h, [1, 2], ["k" => 1]];
foreach ([0, 1, 2, 3, 4, 5, 6, 7, 8] as $i) {
    try {
        echo count($vals[$i]), "|";
    } catch (\TypeError $e) {
        echo substr($e->getMessage(), 63), "|";
    }
}
fclose($h);"#,
    );
    assert_eq!(
        out,
        "int given|string given|float given|true given|false given|null given|resource given|2|1|"
    );
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
