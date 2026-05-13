use super::*;

// PHP's json_encode emits the JSON array form `[...]` whenever the
// associative array's keys form a sequential 0..count-1 sequence in
// insertion order, otherwise the JSON object form `{...}`. elephc's
// runtime now mirrors that behavior via a list-shape pre-detector that
// walks the hash's insertion-order chain.

#[test]
fn test_json_encode_assoc_with_zero_keyed_single_entry_is_list() {
    // [0=>'a'] has the single key 0 in position 0, so it's list-shape.
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a']);"#);
    assert_eq!(out, r#"["a"]"#);
}

#[test]
fn test_json_encode_assoc_with_sequential_zero_keyed_entries_is_list() {
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a', 1=>'b', 2=>'c']);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

#[test]
fn test_json_encode_assoc_starting_at_one_is_object() {
    // Non-zero starting key disqualifies list shape.
    let out = compile_and_run(r#"<?php echo json_encode([1=>'a']);"#);
    assert_eq!(out, r#"{"1":"a"}"#);
}

#[test]
fn test_json_encode_assoc_skipping_keys_is_object() {
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a', 2=>'b']);"#);
    assert_eq!(out, r#"{"0":"a","2":"b"}"#);
}

#[test]
fn test_json_encode_assoc_unordered_keys_is_object() {
    // Insertion order is 2,0,1 — does not match 0,1,2, so object form.
    let out = compile_and_run(r#"<?php echo json_encode([2=>'a', 0=>'b', 1=>'c']);"#);
    assert_eq!(out, r#"{"2":"a","0":"b","1":"c"}"#);
}

#[test]
fn test_json_encode_assoc_with_string_keys_is_object() {
    let out = compile_and_run(r#"<?php echo json_encode(['a'=>1, 'b'=>2]);"#);
    assert_eq!(out, r#"{"a":1,"b":2}"#);
}

#[test]
fn test_json_encode_assoc_mixed_keys_is_object() {
    // Even one string key disqualifies list shape.
    let out = compile_and_run(r#"<?php echo json_encode([0=>'a', 'name'=>'b']);"#);
    assert_eq!(out, r#"{"0":"a","name":"b"}"#);
}

#[test]
fn test_json_encode_force_object_overrides_list_shape() {
    // JSON_FORCE_OBJECT forces object form regardless of list shape.
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>'a', 1=>'b'], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"0":"a","1":"b"}"#);
}

#[test]
fn test_json_encode_indexed_array_still_emits_list() {
    // Pre-existing path — `['a','b']` types as Array<Str> and never
    // reaches the assoc encoder. Verify the indexed-array fast path
    // still produces list form.
    let out = compile_and_run(r#"<?php echo json_encode(['a','b','c']);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

#[test]
fn test_json_encode_assoc_with_int_values_list_shape() {
    let out = compile_and_run(r#"<?php echo json_encode([0=>10, 1=>20, 2=>30]);"#);
    assert_eq!(out, r#"[10,20,30]"#);
}

#[test]
fn test_json_encode_assoc_with_mixed_value_types_list_shape() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>1, 1=>"two", 2=>true, 3=>null]);"#,
    );
    assert_eq!(out, r#"[1,"two",true,null]"#);
}

#[test]
fn test_json_encode_assoc_nested_list_shape() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>[0=>1, 1=>2], 1=>[0=>3, 1=>4]]);"#,
    );
    assert_eq!(out, r#"[[1,2],[3,4]]"#);
}

#[test]
fn test_json_encode_assoc_nested_object_inside_list() {
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>['name'=>'Alice'], 1=>['name'=>'Bob']]);"#,
    );
    assert_eq!(out, r#"[{"name":"Alice"},{"name":"Bob"}]"#);
}

#[test]
fn test_json_encode_assoc_nested_list_inside_object() {
    let out = compile_and_run(
        r#"<?php echo json_encode(['items'=>[0=>'a', 1=>'b']]);"#,
    );
    assert_eq!(out, r#"{"items":["a","b"]}"#);
}

#[test]
fn test_json_encode_force_object_preserved_in_nested_list() {
    // JSON_FORCE_OBJECT is global, so the inner list-shape array also
    // encodes as an object.
    let out = compile_and_run(
        r#"<?php echo json_encode([0=>'a', 1=>'b'], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"0":"a","1":"b"}"#);
}
