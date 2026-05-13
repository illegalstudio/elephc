use super::*;

// --- JSON_UNESCAPED_SLASHES ---

#[test]
fn test_json_encode_default_escapes_slash() {
    let out = compile_and_run(
        r#"<?php echo json_encode("https://example.com/path");"#,
    );
    assert_eq!(out, r#""https:\/\/example.com\/path""#);
}

#[test]
fn test_json_encode_unescaped_slashes_flag() {
    let out = compile_and_run(
        r#"<?php echo json_encode("https://example.com/path", JSON_UNESCAPED_SLASHES);"#,
    );
    assert_eq!(out, r#""https://example.com/path""#);
}

#[test]
fn test_json_encode_unescaped_slashes_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a/b", "c/d"], JSON_UNESCAPED_SLASHES);"#,
    );
    assert_eq!(out, r#"["a/b","c/d"]"#);
}

// --- JSON_PRETTY_PRINT ---

#[test]
fn test_json_encode_pretty_print_indexed_array() {
    let out = compile_and_run(
        "<?php echo json_encode([1, 2, 3], JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[\n    1,\n    2,\n    3\n]");
}

#[test]
fn test_json_encode_pretty_print_assoc() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["name" => "Alice", "age" => 30], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(
        out,
        "{\n    \"name\": \"Alice\",\n    \"age\": 30\n}"
    );
}

#[test]
fn test_json_encode_pretty_print_empty_array_stays_compact() {
    let out = compile_and_run(
        "<?php echo json_encode([], JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[]");
}

#[test]
fn test_json_encode_pretty_print_scalar_unchanged() {
    let out = compile_and_run("<?php echo json_encode(42, JSON_PRETTY_PRINT);");
    assert_eq!(out, "42");
}

#[test]
fn test_json_encode_pretty_print_scalar_string_unchanged() {
    let out = compile_and_run(
        r#"<?php echo json_encode("hello", JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, r#""hello""#);
}

#[test]
fn test_json_encode_pretty_print_nested() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["name" => "Alice", "items" => [1, 2, 3]], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(
        out,
        "{\n    \"name\": \"Alice\",\n    \"items\": [\n        1,\n        2,\n        3\n    ]\n}"
    );
}

#[test]
fn test_json_encode_pretty_print_deeply_nested() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a" => ["b" => ["c" => "deep"]]], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(
        out,
        "{\n    \"a\": {\n        \"b\": {\n            \"c\": \"deep\"\n        }\n    }\n}"
    );
}

#[test]
fn test_json_encode_pretty_print_empty_nested_object() {
    let out = compile_and_run(
        r#"<?php
class O { public int $a = 1; public array $b = []; }
echo json_encode(new O(), JSON_PRETTY_PRINT);
"#,
    );
    assert_eq!(out, "{\n    \"a\": 1,\n    \"b\": []\n}");
}

#[test]
fn test_json_encode_pretty_print_string_with_colon_inside() {
    // Make sure the post-processor leaves colons-inside-strings alone.
    let out = compile_and_run(
        r#"<?php echo json_encode(["url" => "a:b:c"], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, "{\n    \"url\": \"a:b:c\"\n}");
}

#[test]
fn test_json_encode_pretty_print_string_with_brace_inside() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["msg" => "{not a brace}"], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, "{\n    \"msg\": \"{not a brace}\"\n}");
}

#[test]
fn test_json_encode_pretty_print_string_with_escaped_quote() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["q" => "say \"hi\""], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, "{\n    \"q\": \"say \\\"hi\\\"\"\n}");
}

// --- Combined flags ---

#[test]
fn test_json_encode_pretty_print_combined_with_unescaped_slashes() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["url" => "https://x"], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);"#,
    );
    assert_eq!(out, "{\n    \"url\": \"https://x\"\n}");
}

// --- json_last_error stays unaffected by flags ---

#[test]
fn test_json_last_error_after_flag_encoded_call() {
    let out = compile_and_run(
        r#"<?php json_encode(["x"], JSON_PRETTY_PRINT); echo json_last_error();"#,
    );
    assert_eq!(out, "0");
}

// --- JSON_HEX_TAG / JSON_HEX_AMP / JSON_HEX_APOS / JSON_HEX_QUOT ---

#[test]
fn test_json_encode_default_does_not_escape_lt_gt() {
    let out = compile_and_run(r#"<?php echo json_encode("a<b>c");"#);
    assert_eq!(out, r#""a<b>c""#);
}

#[test]
fn test_json_encode_hex_tag_escapes_less_than() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a<b", JSON_HEX_TAG);"#,
    );
    assert_eq!(out, "\"a\\u003Cb\"");
}

#[test]
fn test_json_encode_hex_tag_escapes_greater_than() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a>b", JSON_HEX_TAG);"#,
    );
    assert_eq!(out, "\"a\\u003Eb\"");
}

#[test]
fn test_json_encode_hex_amp_escapes_ampersand() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a&b", JSON_HEX_AMP);"#,
    );
    assert_eq!(out, "\"a\\u0026b\"");
}

#[test]
fn test_json_encode_hex_apos_escapes_apostrophe() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a'b", JSON_HEX_APOS);"#,
    );
    assert_eq!(out, "\"a\\u0027b\"");
}

#[test]
fn test_json_encode_hex_quot_escapes_inner_double_quote() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a\"b", JSON_HEX_QUOT);"#,
    );
    assert_eq!(out, "\"a\\u0022b\"");
}

#[test]
fn test_json_encode_hex_family_combined() {
    let out = compile_and_run(
        r#"<?php echo json_encode("<a>&'\"", JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT);"#,
    );
    assert_eq!(out, "\"\\u003Ca\\u003E\\u0026\\u0027\\u0022\"");
}

#[test]
fn test_json_encode_hex_tag_does_not_affect_amp_or_quote() {
    let out = compile_and_run(
        r#"<?php echo json_encode("<&\">", JSON_HEX_TAG);"#,
    );
    assert_eq!(out, "\"\\u003C&\\\"\\u003E\"");
}

#[test]
fn test_json_encode_hex_quot_preserves_other_escapes() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a\nb\tc\"d", JSON_HEX_QUOT);"#,
    );
    assert_eq!(out, "\"a\\nb\\tc\\u0022d\"");
}

// --- JSON_FORCE_OBJECT ---

#[test]
fn test_json_encode_force_object_int_array() {
    let out = compile_and_run(
        "<?php echo json_encode([1, 2, 3], JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, r#"{"0":1,"1":2,"2":3}"#);
}

#[test]
fn test_json_encode_force_object_string_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a", "b", "c"], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"0":"a","1":"b","2":"c"}"#);
}

#[test]
fn test_json_encode_force_object_empty_array() {
    let out = compile_and_run(
        "<?php echo json_encode([], JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, "{}");
}

#[test]
fn test_json_encode_force_object_single_element() {
    let out = compile_and_run(
        "<?php echo json_encode([42], JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, r#"{"0":42}"#);
}

#[test]
fn test_json_encode_force_object_combined_with_pretty_print() {
    let out = compile_and_run(
        "<?php echo json_encode([10, 20, 30], JSON_FORCE_OBJECT | JSON_PRETTY_PRINT);",
    );
    assert_eq!(
        out,
        "{\n    \"0\": 10,\n    \"1\": 20,\n    \"2\": 30\n}"
    );
}

#[test]
fn test_json_encode_force_object_assoc_array_unaffected() {
    // Already-associative arrays are encoded as objects either way; the flag
    // shouldn't change their existing output.
    let out = compile_and_run(
        r#"<?php echo json_encode(["name" => "Alice"], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"name":"Alice"}"#);
}

#[test]
fn test_json_encode_force_object_does_not_affect_non_array() {
    // Scalars are encoded the same way regardless of FORCE_OBJECT.
    let out = compile_and_run(
        "<?php echo json_encode(42, JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, "42");
}

// --- JSON_PRESERVE_ZERO_FRACTION ---

#[test]
fn test_json_encode_default_drops_zero_fraction() {
    let out = compile_and_run("<?php echo json_encode(1.0);");
    assert_eq!(out, "1");
}

#[test]
fn test_json_encode_preserve_zero_fraction_appends_dot_zero() {
    let out = compile_and_run(
        "<?php echo json_encode(1.0, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "1.0");
}

#[test]
fn test_json_encode_preserve_zero_fraction_keeps_existing_fraction() {
    let out = compile_and_run(
        "<?php echo json_encode(2.5, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "2.5");
}

#[test]
fn test_json_encode_preserve_zero_fraction_zero_value() {
    let out = compile_and_run(
        "<?php echo json_encode(0.0, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "0.0");
}

#[test]
fn test_json_encode_preserve_zero_fraction_negative_one() {
    let out = compile_and_run(
        "<?php echo json_encode(-1.0, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "-1.0");
}

#[test]
fn test_json_encode_preserve_zero_fraction_inside_array() {
    let out = compile_and_run(
        "<?php echo json_encode([1.0, 2.5, 3.0], JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "[1.0,2.5,3.0]");
}

#[test]
fn test_json_encode_preserve_zero_fraction_combined_with_pretty_print() {
    let out = compile_and_run(
        "<?php echo json_encode([1.0, 2.0], JSON_PRESERVE_ZERO_FRACTION | JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[\n    1.0,\n    2.0\n]");
}

#[test]
fn test_json_encode_preserve_zero_fraction_does_not_affect_int() {
    // Integers don't go through __rt_json_encode_float, so the flag is a
    // no-op for them — the encoder still emits the integer literal.
    let out = compile_and_run(
        "<?php echo json_encode(42, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "42");
}

// --- JSON_UNESCAPED_UNICODE ---

#[test]
fn test_json_encode_default_escapes_2byte_utf8() {
    let out = compile_and_run(r#"<?php echo json_encode("café");"#);
    assert_eq!(out, "\"caf\\u00E9\"");
}

#[test]
fn test_json_encode_unescaped_unicode_passes_2byte_utf8() {
    let out = compile_and_run(
        r#"<?php echo json_encode("café", JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#""café""#);
}

#[test]
fn test_json_encode_default_escapes_3byte_utf8() {
    let out = compile_and_run(r#"<?php echo json_encode("你好");"#);
    assert_eq!(out, "\"\\u4F60\\u597D\"");
}

#[test]
fn test_json_encode_unescaped_unicode_passes_3byte_utf8() {
    let out = compile_and_run(
        r#"<?php echo json_encode("你好", JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#""你好""#);
}

#[test]
fn test_json_encode_default_escapes_4byte_utf8_as_surrogate_pair() {
    // 😀 = U+1F600 → high surrogate D83D, low surrogate DE00
    let out = compile_and_run(r#"<?php echo json_encode("😀");"#);
    assert_eq!(out, "\"\\uD83D\\uDE00\"");
}

#[test]
fn test_json_encode_unescaped_unicode_passes_4byte_utf8() {
    let out = compile_and_run(
        r#"<?php echo json_encode("😀", JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#""😀""#);
}

#[test]
fn test_json_encode_default_keeps_ascii_unchanged() {
    // ASCII strings have no multibyte sequences to escape — both modes
    // produce the same output.
    let out = compile_and_run(r#"<?php echo json_encode("hello");"#);
    assert_eq!(out, r#""hello""#);
}

#[test]
fn test_json_encode_default_mixes_ascii_and_escaped_unicode() {
    let out = compile_and_run(r#"<?php echo json_encode("Hi café!");"#);
    assert_eq!(out, "\"Hi caf\\u00E9!\"");
}

#[test]
fn test_json_encode_unescaped_unicode_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["café", "你好"], JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#"["café","你好"]"#);
}

#[test]
fn test_json_encode_unescaped_unicode_inside_assoc() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["greeting" => "你好"], JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#"{"greeting":"你好"}"#);
}

// --- JSON_NUMERIC_CHECK ---

#[test]
fn test_json_encode_default_keeps_numeric_strings_quoted() {
    let out = compile_and_run(r#"<?php echo json_encode("123");"#);
    assert_eq!(out, r#""123""#);
}

#[test]
fn test_json_encode_numeric_check_unquotes_int_string() {
    let out = compile_and_run(
        r#"<?php echo json_encode("123", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_json_encode_numeric_check_unquotes_negative_int() {
    let out = compile_and_run(
        r#"<?php echo json_encode("-5", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "-5");
}

#[test]
fn test_json_encode_numeric_check_unquotes_float() {
    let out = compile_and_run(
        r#"<?php echo json_encode("3.14", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "3.14");
}

#[test]
fn test_json_encode_numeric_check_unquotes_exponent() {
    let out = compile_and_run(
        r#"<?php echo json_encode("1e10", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "1e10");
}

#[test]
fn test_json_encode_numeric_check_unquotes_signed_exponent() {
    let out = compile_and_run(
        r#"<?php echo json_encode("1.5e-3", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "1.5e-3");
}

#[test]
fn test_json_encode_numeric_check_keeps_non_numeric_quoted() {
    let out = compile_and_run(
        r#"<?php echo json_encode("abc", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""abc""#);
}

#[test]
fn test_json_encode_numeric_check_rejects_partial_numeric() {
    let out = compile_and_run(
        r#"<?php echo json_encode("12abc", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""12abc""#);
}

#[test]
fn test_json_encode_numeric_check_rejects_empty_string() {
    let out = compile_and_run(
        r#"<?php echo json_encode("", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""""#);
}

#[test]
fn test_json_encode_numeric_check_rejects_leading_whitespace() {
    let out = compile_and_run(
        r#"<?php echo json_encode(" 123", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#"" 123""#);
}

#[test]
fn test_json_encode_numeric_check_rejects_bare_minus() {
    let out = compile_and_run(
        r#"<?php echo json_encode("-", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""-""#);
}

#[test]
fn test_json_encode_numeric_check_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["1", "2", "abc"], JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#"[1,2,"abc"]"#);
}

#[test]
fn test_json_encode_numeric_check_inside_assoc() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["count" => "42", "name" => "Alice"], JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#"{"count":42,"name":"Alice"}"#);
}
