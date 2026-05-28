//! Purpose:
//! Provides JSON encode flag behavior tests.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Every supported flag must affect output and error state exactly where PHP observes it.

use super::*;

/// Helper that runs `$source` through the system PHP interpreter and returns stdout,
/// or None if PHP is unavailable or the script exits with a non-zero status.
/// Used to obtain a PHP reference fixture for the pretty-print comparison test.
fn php_stdout_for(source: &str) -> Option<String> {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "elephc_php_pretty_fixture_{}_{}.php",
        std::process::id(),
        id
    ));
    fs::write(&path, source).ok()?;
    let output = Command::new("php").arg(&path).output().ok();
    let _ = fs::remove_file(&path);
    let output = output?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

// --- JSON_UNESCAPED_SLASHES ---

/// Verifies that without `JSON_UNESCAPED_SLASHES`, forward slashes are escaped as `\/`.
#[test]
fn test_json_encode_default_escapes_slash() {
    let out = compile_and_run(
        r#"<?php echo json_encode("https://example.com/path");"#,
    );
    assert_eq!(out, r#""https:\/\/example.com\/path""#);
}

/// Verifies that `JSON_UNESCAPED_SLASHES` prevents forward-slash escaping inside strings.
#[test]
fn test_json_encode_unescaped_slashes_flag() {
    let out = compile_and_run(
        r#"<?php echo json_encode("https://example.com/path", JSON_UNESCAPED_SLASHES);"#,
    );
    assert_eq!(out, r#""https://example.com/path""#);
}

/// Verifies `JSON_UNESCAPED_SLASHES` works inside indexed arrays.
#[test]
fn test_json_encode_unescaped_slashes_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a/b", "c/d"], JSON_UNESCAPED_SLASHES);"#,
    );
    assert_eq!(out, r#"["a/b","c/d"]"#);
}

// --- JSON_PRETTY_PRINT ---

/// Verifies `JSON_PRETTY_PRINT` indents a simple indexed array with one level.
#[test]
fn test_json_encode_pretty_print_indexed_array() {
    let out = compile_and_run(
        "<?php echo json_encode([1, 2, 3], JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[\n    1,\n    2,\n    3\n]");
}

/// Verifies `JSON_PRETTY_PRINT` emits an object with key-value pairs on separate lines.
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

/// Verifies that an empty array remains `[]` and is not indented.
#[test]
fn test_json_encode_pretty_print_empty_array_stays_compact() {
    let out = compile_and_run(
        "<?php echo json_encode([], JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[]");
}

/// Verifies that scalar values (non-array/object) are unaffected by `JSON_PRETTY_PRINT`.
#[test]
fn test_json_encode_pretty_print_scalar_unchanged() {
    let out = compile_and_run("<?php echo json_encode(42, JSON_PRETTY_PRINT);");
    assert_eq!(out, "42");
}

/// Verifies that a bare string is returned without extra formatting under `JSON_PRETTY_PRINT`.
#[test]
fn test_json_encode_pretty_print_scalar_string_unchanged() {
    let out = compile_and_run(
        r#"<?php echo json_encode("hello", JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, r#""hello""#);
}

/// Verifies that nested arrays and objects each receive their own indentation level.
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

/// Verifies correct indentation up to at least three levels of nesting.
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

/// Verifies that an empty nested `array` property inside an object remains `[]` and is not omitted.
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

/// Verifies that colons inside string values are not mistaken for JSON structural colons by the post-processor.
#[test]
fn test_json_encode_pretty_print_string_with_colon_inside() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["url" => "a:b:c"], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, "{\n    \"url\": \"a:b:c\"\n}");
}

/// Verifies that braces inside string values are not misinterpreted by the post-processor.
#[test]
fn test_json_encode_pretty_print_string_with_brace_inside() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["msg" => "{not a brace}"], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, "{\n    \"msg\": \"{not a brace}\"\n}");
}

/// Verifies that embedded escaped quotes inside a pretty-printed string are preserved correctly.
#[test]
fn test_json_encode_pretty_print_string_with_escaped_quote() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["q" => "say \"hi\""], JSON_PRETTY_PRINT);"#,
    );
    assert_eq!(out, "{\n    \"q\": \"say \\\"hi\\\"\"\n}");
}

/// Verifies JSON encode pretty print representative payloads match PHP.
#[test]
fn test_json_encode_pretty_print_representative_payloads_match_php() {
    // End-to-end regression test comparing pretty-printed output against the PHP interpreter
    // for a wide variety of payloads: indexed/assoc/nested arrays, objects, JsonSerializable,
    // stdClass, combined flags, and deep nesting. Falls back to a hardcoded fixture if PHP
    // is unavailable.
    let source = r#"<?php
class PrettyPoint { public int $x = 7; public string $label = "pt"; }
class PrettySerializable implements JsonSerializable {
    public function jsonSerialize(): mixed {
        return ["wrapped" => [1, 2], "empty" => []];
    }
}
$o = new stdClass();
$o->name = "Ada";
$o->scores = [1, 2];

echo json_encode([1, 2, 3], JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode(["name" => "Ada", "ok" => true, "none" => null], JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode(["nested" => ["a" => [1, ["b" => 2]]]], JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode([], JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode([1, 2], JSON_PRETTY_PRINT | JSON_FORCE_OBJECT) . "\n---\n";
echo json_encode(["0" => "a", "1" => "b"], JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode(new PrettyPoint(), JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode($o, JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode(new PrettySerializable(), JSON_PRETTY_PRINT) . "\n---\n";
echo json_encode(["chars" => "{a:b}", "quote" => "say \"hi\"", "url" => "https://x/y"], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . "\n---\n";
echo json_encode([[[[[[42]]]]]], JSON_PRETTY_PRINT);
"#;
    let php_fixture = r#"[
    1,
    2,
    3
]
---
{
    "name": "Ada",
    "ok": true,
    "none": null
}
---
{
    "nested": {
        "a": [
            1,
            {
                "b": 2
            }
        ]
    }
}
---
[]
---
{
    "0": 1,
    "1": 2
}
---
[
    "a",
    "b"
]
---
{
    "x": 7,
    "label": "pt"
}
---
{
    "name": "Ada",
    "scores": [
        1,
        2
    ]
}
---
{
    "wrapped": [
        1,
        2
    ],
    "empty": []
}
---
{
    "chars": "{a:b}",
    "quote": "say \"hi\"",
    "url": "https://x/y"
}
---
[
    [
        [
            [
                [
                    [
                        42
                    ]
                ]
            ]
        ]
    ]
]"#;

    let expected = php_stdout_for(source).unwrap_or_else(|| php_fixture.to_string());
    assert_eq!(expected, php_fixture);

    let out = compile_and_run(source);
    assert_eq!(out, expected);
}

/// Verifies that the pretty-print post-processor's indent state is reset after a `JsonException`
/// is thrown, so a subsequent non-throwing `json_encode` call produces correctly-indented output.
#[test]
fn test_json_encode_pretty_print_indent_resets_after_throw() {
    let out = compile_and_run(
        r#"<?php
try {
    json_encode([[[1]]], JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR, 1);
} catch (JsonException $e) {
    echo "caught\n";
}
echo json_encode([1], JSON_PRETTY_PRINT);
"#,
    );
    assert_eq!(out, "caught\n[\n    1\n]");
}

// --- Combined flags ---

/// Verifies that `JSON_PRETTY_PRINT` and `JSON_UNESCAPED_SLASHES` can be combined.
#[test]
fn test_json_encode_pretty_print_combined_with_unescaped_slashes() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["url" => "https://x"], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);"#,
    );
    assert_eq!(out, "{\n    \"url\": \"https://x\"\n}");
}

/// Verifies that `json_last_error()` returns 0 (no error) after a successful `json_encode`
/// call that uses `JSON_PRETTY_PRINT`.
#[test]
fn test_json_last_error_after_flag_encoded_call() {
    let out = compile_and_run(
        r#"<?php json_encode(["x"], JSON_PRETTY_PRINT); echo json_last_error();"#,
    );
    assert_eq!(out, "0");
}

// --- JSON_HEX_TAG / JSON_HEX_AMP / JSON_HEX_APOS / JSON_HEX_QUOT ---

/// Verifies that without `JSON_HEX_TAG`, `<` and `>` are NOT escaped.
#[test]
fn test_json_encode_default_does_not_escape_lt_gt() {
    let out = compile_and_run(r#"<?php echo json_encode("a<b>c");"#);
    assert_eq!(out, r#""a<b>c""#);
}

/// Verifies that `JSON_HEX_TAG` causes `<` to be emitted as `\u003C`.
#[test]
fn test_json_encode_hex_tag_escapes_less_than() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a<b", JSON_HEX_TAG);"#,
    );
    assert_eq!(out, "\"a\\u003Cb\"");
}

/// Verifies that `JSON_HEX_TAG` causes `>` to be emitted as `\u003E`.
#[test]
fn test_json_encode_hex_tag_escapes_greater_than() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a>b", JSON_HEX_TAG);"#,
    );
    assert_eq!(out, "\"a\\u003Eb\"");
}

/// Verifies that `JSON_HEX_AMP` causes `&` to be emitted as `\u0026`.
#[test]
fn test_json_encode_hex_amp_escapes_ampersand() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a&b", JSON_HEX_AMP);"#,
    );
    assert_eq!(out, "\"a\\u0026b\"");
}

/// Verifies that `JSON_HEX_APOS` causes `'` to be emitted as `\u0027`.
#[test]
fn test_json_encode_hex_apos_escapes_apostrophe() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a'b", JSON_HEX_APOS);"#,
    );
    assert_eq!(out, "\"a\\u0027b\"");
}

/// Verifies that `JSON_HEX_QUOT` causes `"` to be emitted as `\u0022`.
#[test]
fn test_json_encode_hex_quot_escapes_inner_double_quote() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a\"b", JSON_HEX_QUOT);"#,
    );
    assert_eq!(out, "\"a\\u0022b\"");
}

/// Verifies that all HEX flags can be combined and each escapes its target character correctly.
#[test]
fn test_json_encode_hex_family_combined() {
    let out = compile_and_run(
        r#"<?php echo json_encode("<a>&'\"", JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT);"#,
    );
    assert_eq!(out, "\"\\u003Ca\\u003E\\u0026\\u0027\\u0022\"");
}

/// Verifies that `JSON_HEX_TAG` only escapes `<` and `>`; other characters are unaffected.
#[test]
fn test_json_encode_hex_tag_does_not_affect_amp_or_quote() {
    let out = compile_and_run(
        r#"<?php echo json_encode("<&\">", JSON_HEX_TAG);"#,
    );
    assert_eq!(out, "\"\\u003C&\\\"\\u003E\"");
}

/// Verifies that `JSON_HEX_QUOT` does not interfere with already-escaped sequences like `\n`, `\t`.
#[test]
fn test_json_encode_hex_quot_preserves_other_escapes() {
    let out = compile_and_run(
        r#"<?php echo json_encode("a\nb\tc\"d", JSON_HEX_QUOT);"#,
    );
    assert_eq!(out, "\"a\\nb\\tc\\u0022d\"");
}

// --- JSON_FORCE_OBJECT ---

/// Verifies that `JSON_FORCE_OBJECT` converts a numeric-indexed array to a JSON object with
/// string keys `"0"`, `"1"`, `"2"`.
#[test]
fn test_json_encode_force_object_int_array() {
    let out = compile_and_run(
        "<?php echo json_encode([1, 2, 3], JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, r#"{"0":1,"1":2,"2":3}"#);
}

/// Verifies that `JSON_FORCE_OBJECT` converts string-valued indexed array elements correctly.
#[test]
fn test_json_encode_force_object_string_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a", "b", "c"], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"0":"a","1":"b","2":"c"}"#);
}

/// Verifies that an empty array with `JSON_FORCE_OBJECT` becomes `{}`, not `[]`.
#[test]
fn test_json_encode_force_object_empty_array() {
    let out = compile_and_run(
        "<?php echo json_encode([], JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, "{}");
}

/// Verifies that a single-element array is encoded as a one-entry object.
#[test]
fn test_json_encode_force_object_single_element() {
    let out = compile_and_run(
        "<?php echo json_encode([42], JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, r#"{"0":42}"#);
}

/// Verifies that `JSON_FORCE_OBJECT` and `JSON_PRETTY_PRINT` can be combined.
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

/// Verifies that already-associative arrays are unaffected by `JSON_FORCE_OBJECT`.
#[test]
fn test_json_encode_force_object_assoc_array_unaffected() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["name" => "Alice"], JSON_FORCE_OBJECT);"#,
    );
    assert_eq!(out, r#"{"name":"Alice"}"#);
}

/// Verifies that scalar values ignore `JSON_FORCE_OBJECT`.
#[test]
fn test_json_encode_force_object_does_not_affect_non_array() {
    let out = compile_and_run(
        "<?php echo json_encode(42, JSON_FORCE_OBJECT);",
    );
    assert_eq!(out, "42");
}

// --- JSON_PRESERVE_ZERO_FRACTION ---

/// Verifies that without `JSON_PRESERVE_ZERO_FRACTION`, `1.0` is encoded as `1`.
#[test]
fn test_json_encode_default_drops_zero_fraction() {
    let out = compile_and_run("<?php echo json_encode(1.0);");
    assert_eq!(out, "1");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` emits `1.0` instead of `1` for whole-number floats.
#[test]
fn test_json_encode_preserve_zero_fraction_appends_dot_zero() {
    let out = compile_and_run(
        "<?php echo json_encode(1.0, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "1.0");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` preserves existing fractional part (no change needed).
#[test]
fn test_json_encode_preserve_zero_fraction_keeps_existing_fraction() {
    let out = compile_and_run(
        "<?php echo json_encode(2.5, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "2.5");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` outputs `0.0` for `0.0`.
#[test]
fn test_json_encode_preserve_zero_fraction_zero_value() {
    let out = compile_and_run(
        "<?php echo json_encode(0.0, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "0.0");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` outputs `-1.0` for negative whole-number floats.
#[test]
fn test_json_encode_preserve_zero_fraction_negative_one() {
    let out = compile_and_run(
        "<?php echo json_encode(-1.0, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "-1.0");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` applies to float values inside arrays.
#[test]
fn test_json_encode_preserve_zero_fraction_inside_array() {
    let out = compile_and_run(
        "<?php echo json_encode([1.0, 2.5, 3.0], JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "[1.0,2.5,3.0]");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` and `JSON_PRETTY_PRINT` can be combined.
#[test]
fn test_json_encode_preserve_zero_fraction_combined_with_pretty_print() {
    let out = compile_and_run(
        "<?php echo json_encode([1.0, 2.0], JSON_PRESERVE_ZERO_FRACTION | JSON_PRETTY_PRINT);",
    );
    assert_eq!(out, "[\n    1.0,\n    2.0\n]");
}

/// Verifies that `JSON_PRESERVE_ZERO_FRACTION` has no effect on integer values.
#[test]
fn test_json_encode_preserve_zero_fraction_does_not_affect_int() {
    let out = compile_and_run(
        "<?php echo json_encode(42, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "42");
}

// --- JSON_UNESCAPED_UNICODE ---

/// Verifies that without `JSON_UNESCAPED_UNICODE`, a 2-byte UTF-8 character (é) is escaped as `\u00E9`.
#[test]
fn test_json_encode_default_escapes_2byte_utf8() {
    let out = compile_and_run(r#"<?php echo json_encode("café");"#);
    assert_eq!(out, "\"caf\\u00E9\"");
}

/// Verifies that `JSON_UNESCAPED_UNICODE` outputs the raw UTF-8 character for 2-byte sequences.
#[test]
fn test_json_encode_unescaped_unicode_passes_2byte_utf8() {
    let out = compile_and_run(
        r#"<?php echo json_encode("café", JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#""café""#);
}

/// Verifies that without `JSON_UNESCAPED_UNICODE`, a 3-byte UTF-8 character (你好) is escaped as `\uXXXX`.
#[test]
fn test_json_encode_default_escapes_3byte_utf8() {
    let out = compile_and_run(r#"<?php echo json_encode("你好");"#);
    assert_eq!(out, "\"\\u4F60\\u597D\"");
}

/// Verifies that `JSON_UNESCAPED_UNICODE` outputs the raw UTF-8 character for 3-byte sequences.
#[test]
fn test_json_encode_unescaped_unicode_passes_3byte_utf8() {
    let out = compile_and_run(
        r#"<?php echo json_encode("你好", JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#""你好""#);
}

/// Verifies that without `JSON_UNESCAPED_UNICODE`, a 4-byte UTF-8 character (😀) is encoded as a
/// UTF-16 surrogate pair (`\uD83D\uDE00`).
#[test]
fn test_json_encode_default_escapes_4byte_utf8_as_surrogate_pair() {
    let out = compile_and_run(r#"<?php echo json_encode("😀");"#);
    assert_eq!(out, "\"\\uD83D\\uDE00\"");
}

/// Verifies that `JSON_UNESCAPED_UNICODE` outputs the raw 4-byte UTF-8 character without surrogate escaping.
#[test]
fn test_json_encode_unescaped_unicode_passes_4byte_utf8() {
    let out = compile_and_run(
        r#"<?php echo json_encode("😀", JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#""😀""#);
}

/// Verifies that ASCII strings are unchanged with or without `JSON_UNESCAPED_UNICODE`.
#[test]
fn test_json_encode_default_keeps_ascii_unchanged() {
    let out = compile_and_run(r#"<?php echo json_encode("hello");"#);
    assert_eq!(out, r#""hello""#);
}

/// Verifies that an ASCII+multibyte string escapes only the multibyte part.
#[test]
fn test_json_encode_default_mixes_ascii_and_escaped_unicode() {
    let out = compile_and_run(r#"<?php echo json_encode("Hi café!");"#);
    assert_eq!(out, "\"Hi caf\\u00E9!\"");
}

/// Verifies that `JSON_UNESCAPED_UNICODE` applies to each element of an indexed array.
#[test]
fn test_json_encode_unescaped_unicode_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["café", "你好"], JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#"["café","你好"]"#);
}

/// Verifies that `JSON_UNESCAPED_UNICODE` applies to values in an associative array.
#[test]
fn test_json_encode_unescaped_unicode_inside_assoc() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["greeting" => "你好"], JSON_UNESCAPED_UNICODE);"#,
    );
    assert_eq!(out, r#"{"greeting":"你好"}"#);
}

// --- JSON_NUMERIC_CHECK ---

/// Verifies that without `JSON_NUMERIC_CHECK`, a string containing digits stays quoted.
#[test]
fn test_json_encode_default_keeps_numeric_strings_quoted() {
    let out = compile_and_run(r#"<?php echo json_encode("123");"#);
    assert_eq!(out, r#""123""#);
}

/// Verifies that `JSON_NUMERIC_CHECK` unquotes a purely integer string.
#[test]
fn test_json_encode_numeric_check_unquotes_int_string() {
    let out = compile_and_run(
        r#"<?php echo json_encode("123", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "123");
}

/// Verifies that `JSON_NUMERIC_CHECK` unquotes a negative integer string.
#[test]
fn test_json_encode_numeric_check_unquotes_negative_int() {
    let out = compile_and_run(
        r#"<?php echo json_encode("-5", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "-5");
}

/// Verifies that `JSON_NUMERIC_CHECK` unquotes a purely float string.
#[test]
fn test_json_encode_numeric_check_unquotes_float() {
    let out = compile_and_run(
        r#"<?php echo json_encode("3.14", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "3.14");
}

/// Verifies that `JSON_NUMERIC_CHECK` unquotes a string in scientific notation (no sign in exponent).
#[test]
fn test_json_encode_numeric_check_unquotes_exponent() {
    let out = compile_and_run(
        r#"<?php echo json_encode("1e10", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "1e10");
}

/// Verifies that `JSON_NUMERIC_CHECK` unquotes a string in scientific notation with signed exponent.
#[test]
fn test_json_encode_numeric_check_unquotes_signed_exponent() {
    let out = compile_and_run(
        r#"<?php echo json_encode("1.5e-3", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, "1.5e-3");
}

/// Verifies that `JSON_NUMERIC_CHECK` leaves a non-numeric string quoted.
#[test]
fn test_json_encode_numeric_check_keeps_non_numeric_quoted() {
    let out = compile_and_run(
        r#"<?php echo json_encode("abc", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""abc""#);
}

/// Verifies that a string with leading digits but a trailing letter is not unquoted.
#[test]
fn test_json_encode_numeric_check_rejects_partial_numeric() {
    let out = compile_and_run(
        r#"<?php echo json_encode("12abc", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""12abc""#);
}

/// Verifies that an empty string is not unquoted by `JSON_NUMERIC_CHECK`.
#[test]
fn test_json_encode_numeric_check_rejects_empty_string() {
    let out = compile_and_run(
        r#"<?php echo json_encode("", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""""#);
}

/// Verifies that a string with leading whitespace is not unquoted by `JSON_NUMERIC_CHECK`.
#[test]
fn test_json_encode_numeric_check_rejects_leading_whitespace() {
    let out = compile_and_run(
        r#"<?php echo json_encode(" 123", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#"" 123""#);
}

/// Verifies that a bare minus sign is not unquoted by `JSON_NUMERIC_CHECK`.
#[test]
fn test_json_encode_numeric_check_rejects_bare_minus() {
    let out = compile_and_run(
        r#"<?php echo json_encode("-", JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#""-""#);
}

/// Verifies that `JSON_NUMERIC_CHECK` applies to each element of an indexed array independently.
#[test]
fn test_json_encode_numeric_check_inside_array() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["1", "2", "abc"], JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#"[1,2,"abc"]"#);
}

/// Verifies that `JSON_NUMERIC_CHECK` applies to associative array values.
#[test]
fn test_json_encode_numeric_check_inside_assoc() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["count" => "42", "name" => "Alice"], JSON_NUMERIC_CHECK);"#,
    );
    assert_eq!(out, r#"{"count":42,"name":"Alice"}"#);
}
