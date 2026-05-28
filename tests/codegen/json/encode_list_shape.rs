//! Purpose:
//! Provides JSON associative-array list-shape tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Sequential integer-keyed hashes should encode as arrays unless forced to objects.

use super::*;

// PHP's json_encode emits the JSON array form `[...]` whenever the
// associative array's keys form a sequential 0..count-1 sequence in
// insertion order, otherwise the JSON object form `{...}`. elephc's
// runtime now mirrors that behavior while it emits the associative-array
// payload, compacting the provisional object bytes when the keys stayed
// list-shaped.

/// Verifies that a single-entry associative array with key 0 emits JSON array form.
/// PHP's json_encode treats `[0=>'a']` as list-shaped and encodes it as `["a"]`.
#[test]
fn test_json_encode_assoc_with_zero_keyed_single_entry_is_list() {
    // [0=>'a'] has the single key 0 in position 0, so it's list-shape.
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a']);"#);
    assert_eq!(out, r#"["a"]"#);
}

/// Verifies that a sequential 0..N key sequence in insertion order emits JSON array form.
/// `[0=>'a', 1=>'b', 2=>'c']` encodes as `["a","b","c"]`.
#[test]
fn test_json_encode_assoc_with_sequential_zero_keyed_entries_is_list() {
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a', 1=>'b', 2=>'c']);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

/// Verifies that a non-zero starting key forces JSON object form.
/// PHP's json_encode treats `[1=>'a']` as an object `{"1":"a"}`, not an array.
#[test]
fn test_json_encode_assoc_starting_at_one_is_object() {
    // Non-zero starting key disqualifies list shape.
    let out = compile_and_run(r#"<?php echo json_encode([1=>'a']);"#);
    assert_eq!(out, r#"{"1":"a"}"#);
}

/// Verifies that a gap in sequential keys forces JSON object form.
/// `[0=>'a', 2=>'b']` has a missing key 1, so it emits `{"0":"a","2":"b"}`.
#[test]
fn test_json_encode_assoc_skipping_keys_is_object() {
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a', 2=>'b']);"#);
    assert_eq!(out, r#"{"0":"a","2":"b"}"#);
}

/// Verifies that insertion-order keys not matching 0..N order force JSON object form.
/// Keys inserted as [2,0,1] do not form a 0..2 sequence, so `{"2":"a","0":"b","1":"c"}` is emitted.
#[test]
fn test_json_encode_assoc_unordered_keys_is_object() {
    // Insertion order is 2,0,1 — does not match 0,1,2, so object form.
    let out = compile_and_run(r#"<?php echo json_encode([2=>'a', 0=>'b', 1=>'c']);"#);
    assert_eq!(out, r#"{"2":"a","0":"b","1":"c"}"#);
}

/// Verifies that string keys always force JSON object form, even when numeric-looking.
/// `['a'=>1, 'b'=>2]` emits `{"a":1,"b":2}`.
#[test]
fn test_json_encode_assoc_with_string_keys_is_object() {
    let out = compile_and_run(r#"<?php echo json_encode(['a'=>1, 'b'=>2]);"#);
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

/// Verifies that a single non-numeric key disqualifies list shape.
/// `[0=>'a', 'name'=>'b']` has a string key, so it emits `{"0":"a","name":"b"}`.
#[test]
fn test_json_encode_assoc_mixed_keys_is_object() {
    // Even one string key disqualifies list shape.
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a', 'name'=>'b']);"#);
    assert_eq!(out, r#"{"0":"a","name":"b"}"#);
}

/// Verifies that JSON_FORCE_OBJECT flag overrides list-shape detection.
/// Even `[0=>'a', 1=>'b']` (normally list-shaped) emits `{"0":"a","1":"b"}`.
#[test]
fn test_json_encode_force_object_overrides_list_shape() {
    // JSON_FORCE_OBJECT forces object form regardless of list shape.
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>'a', 1=>'b'], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"0":"a","1":"b"}"#);
}

/// Verifies that natively indexed PHP arrays (typed Array<Str>) still encode as JSON arrays.
/// `['a','b','c']` types as an indexed array and must not hit the assoc encoder path.
#[test]
fn test_json_encode_indexed_array_still_emits_list() {
    // Pre-existing path — `['a','b']` types as Array<Str> and never
    // reaches the assoc encoder. Verify the indexed-array fast path
    // still produces list form.
    let out = compile_and_run(r#"<?php echo json_encode(['a','b','c']);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

/// Verifies integer values in a list-shape associative array encode correctly as JSON array.
/// `[0=>10, 1=>20, 2=>30]` emits `[10,20,30]`.
#[test]
fn test_json_encode_assoc_with_int_values_list_shape() {
    let out = compile_and_run(r#"<?php echo json_encode([0=>10, 1=>20, 2=>30]);"#);
    assert_eq!(out, r#"[10,20,30]"#);
}

/// Verifies mixed PHP value types (int, string, bool, null) encode correctly in list form.
/// `[0=>1, 1=>"two", 2=>true, 3=>null]` emits `[1,"two",true,null]`.
#[test]
fn test_json_encode_assoc_with_mixed_value_types_list_shape() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>1, 1=>"two", 2=>true, 3=>null]);"#,
    );
    assert_eq!(out, r#"[1,"two",true,null]"#);
}

/// Verifies that nested list-shape arrays inside a list-shape parent emit as nested JSON arrays.
/// `[[1,2],[3,4]]` — both inner and outer arrays are list-shaped.
#[test]
fn test_json_encode_assoc_nested_list_shape() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>[0=>1, 1=>2], 1=>[0=>3, 1=>4]]);"#,
    );
    assert_eq!(out, r#"[[1,2],[3,4]]"#);
}

/// Verifies that list-shape parent arrays containing object-shaped nested arrays emit correctly.
/// The outer `[0=>[...], 1=>[...]]` is list-shaped; inner arrays have string keys and emit as objects.
#[test]
fn test_json_encode_assoc_nested_object_inside_list() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>['name'=>'Alice'], 1=>['name'=>'Bob']]);"#,
    );
    assert_eq!(out, r#"[{"name":"Alice"},{"name":"Bob"}]"#);
}

/// Verifies that string delimiters (brackets, braces, quotes, backslashes) are escaped safely
/// inside list-shape output and do not corrupt the JSON structure.
#[test]
fn test_json_encode_assoc_list_shape_compacts_string_delimiters_safely() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>"a,}]b", 1=>"quote\"slash\\", 2=>["x"=>"{,}"]]);"#,
    );
    assert_eq!(out, r#"["a,}]b","quote\"slash\\",{"x":"{,}"}]"#);
}

/// Verifies that a string-keyed value containing a list-shape array emits correctly.
/// Outer object `{'items'=>['a','b']}` encodes as `{"items":["a","b"]}`.
#[test]
fn test_json_encode_assoc_nested_list_inside_object() {
    let out = compile_and_run(
        r#"<?php echo json_encode(['items'=>[0=>'a', 1=>'b']]);"#,
    );
    assert_eq!(out, r#"{"items":["a","b"]}"#);
}

/// Verifies that JSON_FORCE_OBJECT flag propagates to all nested list-shape arrays.
/// With the flag set, even `[0=>'a', 1=>'b']` encodes as `{"0":"a","1":"b"}`, not an array.
#[test]
fn test_json_encode_force_object_preserved_in_nested_list() {
    // JSON_FORCE_OBJECT is global, so the inner list-shape array also
    // encodes as an object.
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>'a', 1=>'b'], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"0":"a","1":"b"}"#);
}
