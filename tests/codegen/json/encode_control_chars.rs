use super::*;

// JSON requires every control byte (< 0x20) inside a string to be escaped.
// elephc's encoder uses the canonical short escapes for the five RFC-named
// controls (\b, \t, \n, \f, \r) and emits \uXXXX for every other control
// byte. The malformed inputs are constructed via chr() since elephc's
// lexer does not parse \xHH string escapes.

// Verify per-byte escape dispatch for every distinct control-byte class:
//   * 0x00..0x07 / 0x0B / 0x0E..0x1F → generic unicode escape
//   * 0x08 (\b) and 0x0C (\f) → canonical short escapes
// Originally these were 8 separate tests (one per representative byte);
// merged here into a single multi-echo so the dispatch is exercised in
// one compile/link/run cycle. The mixed-byte test below still verifies
// that short and unicode escapes can interleave in one string.
#[test]
fn test_json_encode_control_byte_dispatch() {
    let out = compile_and_run(
        r#"<?php
echo json_encode("a" . chr(0x00) . "b") . "\n";
echo json_encode("a" . chr(0x01) . "b") . "\n";
echo json_encode("a" . chr(0x08) . "b") . "\n";
echo json_encode("a" . chr(0x0B) . "b") . "\n";
echo json_encode("a" . chr(0x0C) . "b") . "\n";
echo json_encode("a" . chr(0x0E) . "b") . "\n";
echo json_encode("a" . chr(0x1F) . "b");
"#,
    );
    let expected = "\"a\\u0000b\"\n\"a\\u0001b\"\n\"a\\bb\"\n\"a\\u000Bb\"\n\"a\\fb\"\n\"a\\u000Eb\"\n\"a\\u001Fb\"";
    assert_eq!(out, expected);
}

#[test]
fn test_json_encode_space_byte_remains_literal() {
    // 0x20 (space) is the first byte that is NOT a control character and
    // must be copied as-is.
    let out = compile_and_run(
        r#"<?php echo json_encode("a" . chr(0x20) . "b");"#,
    );
    assert_eq!(out, r#""a b""#);
}

#[test]
fn test_json_encode_multiple_control_bytes() {
    // Mix of canonical short escapes and generic \u00XX escapes in one
    // string verifies the escape dispatch handles both branches.
    let out = compile_and_run(
        r#"<?php echo json_encode(chr(0x00) . chr(0x08) . chr(0x09) . chr(0x0A) . chr(0x0B));"#,
    );
    assert_eq!(out, "\"\\u0000\\b\\t\\n\\u000B\"");
}

#[test]
fn test_json_encode_control_byte_inside_array() {
    // The dispatcher routes through __rt_json_encode_str for every array
    // element string, so the control-byte escaping must apply uniformly.
    let out = compile_and_run(
        r#"<?php echo json_encode(["pre", chr(0x01), "post"]);"#,
    );
    assert_eq!(out, "[\"pre\",\"\\u0001\",\"post\"]");
}

#[test]
fn test_json_encode_control_byte_in_assoc_value() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["k" => "x" . chr(0x07) . "y"]);"#,
    );
    assert_eq!(out, "{\"k\":\"x\\u0007y\"}");
}

#[test]
fn test_json_encode_tab_in_assoc_key() {
    // JSON object keys must be valid JSON strings: a tab inside a key
    // must be escaped as \t.
    let out = compile_and_run(
        r#"<?php echo json_encode(["k" . chr(0x09) . "v" => 1]);"#,
    );
    assert_eq!(out, "{\"k\\tv\":1}");
}

#[test]
fn test_json_encode_quote_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a\"b" => 1]);"#,
    );
    assert_eq!(out, "{\"a\\\"b\":1}");
}

#[test]
fn test_json_encode_control_byte_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a" . chr(0x01) . "b" => 2]);"#,
    );
    assert_eq!(out, "{\"a\\u0001b\":2}");
}

#[test]
fn test_json_encode_backslash_in_assoc_key() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["a\\b" => 3]);"#,
    );
    assert_eq!(out, "{\"a\\\\b\":3}");
}

#[test]
fn test_json_encode_integer_assoc_key_unchanged() {
    // Integer keys are formatted as decimal digits and never need escape;
    // verify the integer-key path still works after the assoc-key
    // refactor.
    let out = compile_and_run(
        r#"<?php echo json_encode([42 => "x"]);"#,
    );
    assert_eq!(out, r#"{"42":"x"}"#);
}
