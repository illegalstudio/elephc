//! Purpose:
//! Covers the remaining PHP JSON constants and their observable encoder behavior.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext json::compatibility_constants`.
//!
//! Key details:
//! - Fixed PHP-compatible expectations cover native compilation; interpreter tests cover eval.
//! - The line-terminator flag only applies together with unescaped Unicode.

use super::*;

/// Both missing constants must be visible through namespaced native lookup APIs.
#[test]
fn test_json_compatibility_constants_lookup() {
    let out = compile_and_run(
        r#"<?php
namespace JsonConstants;
echo \JSON_UNESCAPED_LINE_TERMINATORS . ":" . JSON_ERROR_NON_BACKED_ENUM . "\n";
echo constant("JSON_UNESCAPED_LINE_TERMINATORS") . ":" . constant("JSON_ERROR_NON_BACKED_ENUM") . "\n";
echo defined("JSON_UNESCAPED_LINE_TERMINATORS") ? "Y" : "N";
echo defined("JSON_ERROR_NON_BACKED_ENUM") ? "Y" : "N";
echo "\n";
"#,
    );
    assert_eq!(out, "2048:11\n2048:11\nYY\n");
}

/// Separators stay escaped unless both unescaped flags are set, including nested keys.
#[test]
fn test_json_unescaped_line_terminators_native() {
    let body = r#"
$text = "é\u{2028}\u{2029}😀";
foreach ([0, JSON_UNESCAPED_LINE_TERMINATORS, JSON_UNESCAPED_UNICODE,
          JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_LINE_TERMINATORS] as $flags) {
    echo json_encode($text, $flags) . "\n";
    echo json_encode([$text => [$text]], $flags) . "\n";
}
"#;
    let source = format!("<?php\n{body}");
    let escaped = r#""\u00e9\u2028\u2029\ud83d\ude00""#;
    let unicode = "\"é\\u2028\\u2029😀\"";
    let literal = "\"é\u{2028}\u{2029}😀\"";
    let expected: String = [escaped, escaped, unicode, literal].iter()
        .map(|text| format!("{text}\n{{{text}:[{text}]}}\n"))
        .collect();
    assert_eq!(compile_and_run(&source), expected);
}

/// UTF-8 sanitization still respects the line-terminator flags for valid scalars.
#[test]
fn test_json_unescaped_line_terminators_with_utf8_flags() {
    let out = compile_and_run(
        r#"<?php
$text = "x\u{2028}\u{2029}y";
foreach ([JSON_INVALID_UTF8_IGNORE, JSON_INVALID_UTF8_SUBSTITUTE] as $sanitize) {
    echo json_encode($text, $sanitize | JSON_UNESCAPED_UNICODE) . "\n";
    echo json_encode($text, $sanitize | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_LINE_TERMINATORS) . "\n";
}
"#,
    );
    assert_eq!(out, "\"x\\u2028\\u2029y\"\n\"x\u{2028}\u{2029}y\"\n".repeat(2));
}

/// Unit enum encoding reports code 11, uses zero in partial output, and can throw.
#[test]
fn test_json_non_backed_enum_errors_native() {
    let body = r#"
enum JsonUnit { case Ready; }
var_dump(json_encode(JsonUnit::Ready));
echo json_last_error() . ":" . json_last_error_msg() . "\n";
echo json_encode([JsonUnit::Ready], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":" . json_last_error() . "\n";
echo json_encode(["state" => JsonUnit::Ready], JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_THROW_ON_ERROR) . "\n";
try {
    json_encode(JsonUnit::Ready, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo $e->getCode() . ":" . $e->getMessage() . "\n";
}
echo json_encode("ok") . ":" . json_last_error() . "\n";
"#;
    let source = format!("<?php{body}");
    let expected = "bool(false)\n11:Non-backed enums have no default serialization\n[0]:11\n{\"state\":0}\n11:Non-backed enums have no default serialization\n\"ok\":0\n";
    assert_eq!(compile_and_run(&source), expected);

}

/// A custom JsonSerializable implementation retains precedence over the unit enum error.
#[test]
fn test_json_non_backed_enum_custom_serialization() {
    let out = compile_and_run(
        r#"<?php
enum JsonCustomUnit implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { return "ready"; }
}
echo json_encode(JsonCustomUnit::Ready, JSON_THROW_ON_ERROR) . ":" . json_last_error();
"#,
    );
    assert_eq!(out, "\"ready\":0");
}

/// Backed enums encode as their scalar values, including boxed values nested in containers.
#[test]
fn test_json_backed_enum_scalar_values() {
    let out = compile_and_run(r#"<?php
enum JsonIntCase: int { case N = 7; case Zero = 0; case Negative = -3; }
enum JsonStringCase: string { case Hi = "hi"; case Numeric = "12"; case Escaped = "a/b"; }
echo json_encode(JsonIntCase::N) . ":" . json_encode(JsonIntCase::Zero) . ":" . json_encode(JsonIntCase::Negative) . "\n";
echo json_encode(JsonStringCase::Hi) . ":" . json_encode(JsonStringCase::Numeric) . "\n";
echo json_encode([JsonIntCase::N, JsonStringCase::Hi, ["case" => JsonIntCase::Zero]]) . "\n";
echo json_encode(["case" => JsonStringCase::Escaped], JSON_UNESCAPED_SLASHES) . "\n";
echo json_encode(JsonStringCase::Numeric, JSON_NUMERIC_CHECK) . ":" . json_last_error() . "\n";
echo json_encode([JsonIntCase::N, JsonStringCase::Hi], 0, 1) . ":" . json_last_error();
"#);
    assert_eq!(out, "7:0:-3\n\"hi\":\"12\"\n[7,\"hi\",{\"case\":0}]\n{\"case\":\"a/b\"}\n12:0\n[7,\"hi\"]:0");
}

/// JsonSerializable overrides a backed enum's stored scalar value.
#[test]
fn test_json_backed_enum_custom_serialization() {
    let out = compile_and_run(r#"<?php
enum JsonCustomIntCase: int implements JsonSerializable {
    case N = 7;
    public function jsonSerialize(): mixed { return "ready"; }
}
echo json_encode(JsonCustomIntCase::N) . "\n";
echo json_encode(["case" => JsonCustomIntCase::N], JSON_THROW_ON_ERROR) . ":" . json_last_error();
"#);
    assert_eq!(out, "\"ready\"\n{\"case\":\"ready\"}:0");
}

/// Self-returning enum serializers expose properties once and consume one object depth.
#[test]
fn test_json_enum_serializer_self_returns_native() {
    let source = r#"<?php
enum JsonSelfUnit implements JsonSerializable {
    case Ready;
    public function jsonSerialize(): mixed { echo "self:"; return $this; }
}
enum JsonSelfInt: int implements JsonSerializable {
    case N = 7;
    public function jsonSerialize(): mixed { return $this; }
}
enum JsonSelfString: string implements JsonSerializable {
    case Hi = "hi";
    public function jsonSerialize(): mixed { return $this; }
}
foreach ([JsonSelfUnit::Ready, JsonSelfInt::N, JsonSelfString::Hi] as $case) {
    try { $encoded = json_encode($case, JSON_THROW_ON_ERROR, 1); }
    catch (JsonException $e) { echo "unexpected:" . $e->getCode(); exit(0); }
    echo $encoded . "\n";
    echo json_encode([$case], 0, 2) . "\n";
    echo json_encode($case, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR, 1) . "\n";
}
echo json_last_error();
"#;
    let expected = concat!(
        "self:{\"name\":\"Ready\"}\nself:[{\"name\":\"Ready\"}]\nself:{\n    \"name\": \"Ready\"\n}\n",
        "{\"name\":\"N\",\"value\":7}\n[{\"name\":\"N\",\"value\":7}]\n{\n    \"name\": \"N\",\n    \"value\": 7\n}\n",
        "{\"name\":\"Hi\",\"value\":\"hi\"}\n[{\"name\":\"Hi\",\"value\":\"hi\"}]\n{\n    \"name\": \"Hi\",\n    \"value\": \"hi\"\n}\n",
        "0",
    );
    let run = || {
        assert_eq!(compile_and_run(source), expected);
        let debug = compile_and_run_with_heap_debug(source);
        assert!(debug.success, "{}", debug.stderr);
        assert_eq!(debug.stdout, expected);
    };
    run();
    without_ir_opt(run);
}
