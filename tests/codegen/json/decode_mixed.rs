//! Purpose:
//! Provides structural JSON decode tests for Mixed payloads.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Every JSON value kind should round-trip through the boxed Mixed representation.

use super::*;

/// json_decode returns a structural Mixed cell. When echoed, PHP's type-juggling
/// rules govern the printed bytes (true → "1", false → "", null → "",
/// int/float → decimal, string → bytes). This test verifies gettype() on every
/// scalar JSON literal (integer, double, boolean×2, NULL, string).
///
/// Originally 6 separate one-line tests merged into one compile/link/run cycle
/// to save fork+exec overhead; a single failed assertion still pinpoints the
/// offending scalar via the joined-output diff.
#[test]
fn test_json_decode_gettype_per_scalar() {
    let out = compile_and_run(
        r#"<?php
echo gettype(json_decode("42")) . "\n";
echo gettype(json_decode("3.14")) . "\n";
echo gettype(json_decode("true")) . "\n";
echo gettype(json_decode("false")) . "\n";
echo gettype(json_decode("null")) . "\n";
echo gettype(json_decode("\"hello\""));
"#,
    );
    assert_eq!(out, "integer\ndouble\nboolean\nboolean\nNULL\nstring");
}

/// Verifies intval() can lift a Mixed int payload back to a typed integer for arithmetic.
#[test]
fn test_json_decode_int_value_preserved() {
    // intval() lifts a Mixed payload back to a typed Int so the value can
    // participate in arithmetic — elephc's type system requires numeric
    // operands for `+` and Mixed alone does not satisfy the contract.
    let out = compile_and_run(r#"<?php $x = json_decode("100"); echo intval($x) + 5;"#);
    assert_eq!(out, "105");
}

/// Verifies json_decode("-42") returns a negative integer.
#[test]
fn test_json_decode_negative_int() {
    let out = compile_and_run(r#"<?php $x = json_decode("-42"); echo $x;"#);
    assert_eq!(out, "-42");
}

/// Verifies json_decode("0") returns integer type with value 0.
#[test]
fn test_json_decode_zero() {
    let out = compile_and_run(r#"<?php $x = json_decode("0"); echo gettype($x) . ":" . $x;"#);
    assert_eq!(out, "integer:0");
}

/// Verifies json_decode("2.5") returns a float with fraction preserved.
#[test]
fn test_json_decode_float_with_fraction() {
    let out = compile_and_run(r#"<?php $x = json_decode("2.5"); echo $x;"#);
    assert_eq!(out, "2.5");
}

/// Verifies json_decode("-1.25") returns a negative float.
#[test]
fn test_json_decode_negative_float() {
    let out = compile_and_run(r#"<?php $x = json_decode("-1.25"); echo $x;"#);
    assert_eq!(out, "-1.25");
}

/// Verifies json_decode("1.5e2") parses exponent notation and returns 150.0 as float.
#[test]
fn test_json_decode_float_with_exponent() {
    let out = compile_and_run(r#"<?php $x = json_decode("1.5e2"); echo $x;"#);
    assert_eq!(out, "150");
}

/// Verifies json_decode("true") echoes as "1" (PHP bool→string cast: true → "1").
#[test]
fn test_json_decode_true_echoes_as_one() {
    // PHP rule: bool→string casts true to "1".
    let out = compile_and_run(r#"<?php echo json_decode("true");"#);
    assert_eq!(out, "1");
}

/// Verifies json_decode("false") echoes as "" (PHP bool→string cast: false → "").
#[test]
fn test_json_decode_false_echoes_as_empty() {
    // PHP rule: bool→string casts false to "" (empty string).
    let out = compile_and_run(r#"<?php echo json_decode("false");"#);
    assert_eq!(out, "");
}

/// Verifies json_decode("null") echoes as "" (PHP null→string cast → "").
#[test]
fn test_json_decode_null_echoes_as_empty() {
    // PHP rule: null→string casts to "" (empty string).
    let out = compile_and_run(r#"<?php echo json_decode("null");"#);
    assert_eq!(out, "");
}

/// Verifies json_decode("\"hello\"") returns the string content "hello".
#[test]
fn test_json_decode_string_echoes_as_content() {
    let out = compile_and_run(r#"<?php echo json_decode("\"hello\"");"#);
    assert_eq!(out, "hello");
}

/// Verifies a string with an escape sequence decodes correctly; strlen reports 8 bytes
/// for "hi\nthere" (h,i,\n,t,h,e,r,e).
#[test]
fn test_json_decode_string_with_escape() {
    let out = compile_and_run(
        r#"<?php $s = json_decode("\"hi\\nthere\""); echo strlen($s);"#,
    );
    // "hi\nthere" = 8 bytes: h, i, \n, t, h, e, r, e
    assert_eq!(out, "8");
}

/// Verifies json_decode("[1, 2, 3]") returns a Mixed(array) type observable via gettype().
#[test]
fn test_json_decode_array_is_structural() {
    // Non-empty arrays now decode structurally via the recursive
    // value-producing parser. Each element is recursively decoded and
    // pushed as a Mixed pointer into a fresh __rt_array_new.
    let out = compile_and_run(r#"<?php $x = json_decode(" [1, 2, 3] "); echo gettype($x);"#);
    assert_eq!(out, "array");
}

/// Verifies json_decode("{\"a\": 1}") returns Mixed(object) by default and Mixed(array)
/// when assoc=true. Both are structural (not just scalars).
#[test]
fn test_json_decode_object_is_structural() {
    // Non-empty objects now decode structurally via the recursive parser.
    // Each key is parsed as a JSON string; each value recursively decodes
    // and is inserted into the destination hash via __rt_hash_set. PHP's
    // default returns stdClass; passing assoc=true forces an associative
    // array instead.
    let stdclass = compile_and_run(
        r#"<?php $x = json_decode(" {\"a\": 1} "); echo gettype($x);"#,
    );
    assert_eq!(stdclass, "object");
    let assoc = compile_and_run(
        r#"<?php $x = json_decode(" {\"a\": 1} ", true); echo gettype($x);"#,
    );
    assert_eq!(assoc, "array");
}

/// Verifies json_decode("[]") returns a real empty Mixed(array) cell (gettype=array).
#[test]
fn test_json_decode_empty_array_is_structural() {
    // [] with no content (or only whitespace) decodes to a real empty
    // Mixed(array) cell — observable via gettype() and round-tripped
    // through json_encode as `[]`.
    let out = compile_and_run(r#"<?php echo gettype(json_decode("[]"));"#);
    assert_eq!(out, "array");
}

/// Verifies whitespace-only JSON ("[ \t\n ]") still decodes to a structural empty array.
#[test]
fn test_json_decode_empty_array_with_whitespace_is_structural() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("[ \t\n ]"));"#);
    assert_eq!(out, "array");
}

/// Verifies json_decode("{}") returns a stdClass instance by default (object type).
#[test]
fn test_json_decode_empty_object_is_structural() {
    // PHP default: empty object decodes to a stdClass instance.
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}"));"#);
    assert_eq!(out, "object");
}

/// Verifies whitespace inside empty object still decodes to object type.
#[test]
fn test_json_decode_empty_object_with_whitespace_is_structural() {
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{   }"));"#);
    assert_eq!(out, "object");
}

/// Verifies assoc=true coerces an empty object to an associative array (gettype=array).
#[test]
fn test_json_decode_empty_object_assoc_is_array() {
    // assoc=true coerces the empty object into an associative array.
    let out = compile_and_run(r#"<?php echo gettype(json_decode("{}", true));"#);
    assert_eq!(out, "array");
}

/// Verifies an empty array round-trips through json_encode as [].
#[test]
fn test_json_decode_empty_array_round_trips() {
    let out = compile_and_run(r#"<?php echo json_encode(json_decode("[]"));"#);
    assert_eq!(out, "[]");
}

/// Verifies an empty stdClass round-trips through json_encode as {}.
#[test]
fn test_json_decode_empty_object_round_trips() {
    // PHP default: empty object decodes to a stdClass and re-encodes as `{}`.
    let out = compile_and_run(r#"<?php echo json_encode(json_decode("{}"));"#);
    assert_eq!(out, "{}");
}

/// Verifies assoc=true empty object round-trips as [] (PHP's list-shape detection renders
/// empty associative array as array form).
#[test]
fn test_json_decode_empty_object_assoc_round_trips() {
    // assoc=true forces an empty associative array; PHP's list-shape
    // detection then renders it as `[]`.
    let out = compile_and_run(r#"<?php echo json_encode(json_decode("{}", true));"#);
    assert_eq!(out, "[]");
}

// --- Recursive array decode (non-empty arrays) ---

/// Verifies a non-empty integer array decodes structurally and round-trips through json_encode.
#[test]
fn test_json_decode_array_of_ints_round_trips() {
    let out = compile_and_run(r#"<?php echo json_encode(json_decode("[1, 2, 3]"));"#);
    assert_eq!(out, "[1,2,3]");
}

/// Verifies a string array decodes structurally and round-trips.
#[test]
fn test_json_decode_array_of_strings_round_trips() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[\"a\", \"b\", \"c\"]"));"#,
    );
    assert_eq!(out, r#"["a","b","c"]"#);
}

/// Verifies an array with mixed JSON value types round-trips through json_encode.
#[test]
fn test_json_decode_array_of_mixed_scalars_round_trips() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[1, \"two\", 3.14, true, false, null]"));"#,
    );
    assert_eq!(out, r#"[1,"two",3.14,true,false,null]"#);
}

/// Verifies nested arrays ([1, [2, 3], 4]) decode structurally and round-trip as-is.
#[test]
fn test_json_decode_nested_arrays_round_trip() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[1, [2, 3], 4]"));"#,
    );
    assert_eq!(out, "[1,[2,3],4]");
}

/// Verifies deeply nested arrays round-trip correctly.
#[test]
fn test_json_decode_deeply_nested_arrays() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[[[1, 2], [3, 4]], [[5, 6], [7, 8]]]"));"#,
    );
    assert_eq!(out, "[[[1,2],[3,4]],[[5,6],[7,8]]]");
}

/// Verifies strings containing comma, ], [ do not confuse the boundary scanner during decode.
#[test]
fn test_json_decode_array_with_strings_containing_special_chars() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[\"a,b\", \"]\", \"[\"]"));"#,
    );
    // Strings inside the array contain comma and brackets that the boundary
    // scanner must NOT treat as element separators or container delimiters.
    assert_eq!(out, r#"["a,b","]","["]"#);
}

/// Verifies an escaped quote inside an array element string is handled by the inner recursive
/// decode of the element string, not skipped by the boundary scanner.
#[test]
fn test_json_decode_array_with_escaped_quote_in_string() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[\"hi\\\"there\"]"));"#,
    );
    // The escape inside the string MUST be handled by the inner recursive
    // decode of the element string, not skipped by the boundary scanner.
    assert_eq!(out, r#"["hi\"there"]"#);
}

// Note: `count($x)` and `$x[i]` on a Mixed payload are not yet supported
// by the type checker (count() requires Array<_>, indexing requires
// Array/AssocArray). Decoded arrays still round-trip cleanly through
// json_encode and the array helpers internally, but typed-builtin
// access requires a Mixed-aware path that mirrors the strlen/intval
// relaxation work in a follow-up.

/// Verifies whitespace padding around array elements does not affect decode or encode.
#[test]
fn test_json_decode_array_with_whitespace_round_trips() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("  [  1  ,  2  ,  3  ]  "));"#,
    );
    assert_eq!(out, "[1,2,3]");
}

/// Verifies a single-element array decodes and round-trips correctly.
#[test]
fn test_json_decode_single_element_array() {
    let out = compile_and_run(r#"<?php echo json_encode(json_decode("[42]"));"#);
    assert_eq!(out, "[42]");
}

// --- Recursive object decode (non-empty objects) ---

/// Verifies a simple object decodes structurally and round-trips as-is.
#[test]
fn test_json_decode_simple_object_round_trips() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a\": 1, \"b\": 2}"));"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

/// Verifies an object with string values round-trips correctly.
#[test]
fn test_json_decode_object_with_string_values() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"name\": \"Alice\", \"city\": \"Paris\"}"));"#,
    );
    assert_eq!(out, r#"{"name":"Alice","city":"Paris"}"#);
}

/// Verifies an object with mixed value types (int, float, bool, null, string) round-trips.
#[test]
fn test_json_decode_object_with_mixed_value_types() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"int\": 42, \"float\": 3.14, \"bool\": true, \"null\": null, \"str\": \"hi\"}"));"#,
    );
    assert_eq!(
        out,
        r#"{"int":42,"float":3.14,"bool":true,"null":null,"str":"hi"}"#
    );
}

/// Verifies nested objects decode structurally and round-trip as-is.
#[test]
fn test_json_decode_nested_object_round_trips() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"outer\": {\"inner\": 42}}"));"#,
    );
    assert_eq!(out, r#"{"outer":{"inner":42}}"#);
}

/// Verifies an object with an array-valued property round-trips correctly.
#[test]
fn test_json_decode_object_with_array_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"list\": [1, 2, 3]}"));"#,
    );
    assert_eq!(out, r#"{"list":[1,2,3]}"#);
}

/// Verifies an array of objects round-trips correctly.
#[test]
fn test_json_decode_array_of_objects() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("[{\"a\":1},{\"a\":2}]"));"#,
    );
    assert_eq!(out, r#"[{"a":1},{"a":2}]"#);
}

/// Verifies a key containing '"' and value containing comma/braces does not confuse the
/// boundary scanner.
#[test]
fn test_json_decode_object_with_string_containing_special_chars() {
    // Key contains '"' (escaped) and value contains ',' / '{' / '}'.
    // Both must NOT confuse the boundary scanner.
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"a,b\": \"{value}\"}"));"#,
    );
    assert_eq!(out, r#"{"a,b":"{value}"}"#);
}

/// Verifies a complex nested payload (users array with nested objects) round-trips correctly.
#[test]
fn test_json_decode_complex_nested_payload() {
    let out = compile_and_run(
        r#"<?php
$json = "{\"users\": [{\"name\": \"Alice\", \"age\": 30}, {\"name\": \"Bob\", \"age\": 25}], \"count\": 2}";
echo json_encode(json_decode($json));
"#,
    );
    assert_eq!(
        out,
        r#"{"users":[{"name":"Alice","age":30},{"name":"Bob","age":25}],"count":2}"#
    );
}

/// Verifies whitespace padding around object keys/values does not affect decode or encode.
#[test]
fn test_json_decode_object_with_whitespace_round_trips() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("  {  \"a\"  :  1  ,  \"b\"  :  2  }  "));"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

/// Verifies a key with escaped newlines decodes correctly; on re-encode the newlines get the
/// canonical \n escape form.
#[test]
fn test_json_decode_object_with_escaped_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(json_decode("{\"key\\nwith\\nnewlines\": 1}"));"#,
    );
    // The key contains real newlines after escape decoding; on re-encode
    // they get the canonical \n escape.
    assert_eq!(out, r#"{"key\nwith\nnewlines":1}"#);
}

/// Verifies whitespace-only input ("   ") returns NULL with no error set.
#[test]
fn test_json_decode_whitespace_only_returns_null() {
    let out = compile_and_run(r#"<?php $x = json_decode("   "); echo gettype($x);"#);
    assert_eq!(out, "NULL");
}

/// Verifies json_decode("-0") returns an integer (not float) matching PHP's treatment of "-0".
#[test]
fn test_json_decode_with_leading_minus_zero_is_int() {
    let out = compile_and_run(r#"<?php $x = json_decode("-0"); echo gettype($x);"#);
    assert_eq!(out, "integer");
}

/// Verifies intval() can lift two Mixed int payloads for arithmetic; round-trip from
/// JSON literal → Mixed → int → sum.
#[test]
fn test_json_decode_int_then_arithmetic() {
    // intval() coerces both Mixed payloads to Int before the arithmetic;
    // verifies the round-trip from JSON literal → Mixed → int → sum.
    let out = compile_and_run(
        r#"<?php $x = json_decode("10"); $y = json_decode("32"); echo intval($x) + intval($y);"#,
    );
    assert_eq!(out, "42");
}
