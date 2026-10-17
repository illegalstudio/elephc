//! Purpose:
//! Regressions for PHP 8.6 JSON token coordinates in native decode and validate.
//!
//! Called from:
//! - The JSON codegen integration suite.
//!
//! Key details:
//! - Both optimizer modes and heap-debug execution share the same expectations.
//! - A requested PHP 8.6 oracle compares complete messages, including coordinates.

use super::diagnostic_regressions::assert_json_diagnostics_since;

/// Structural, trailing, unterminated-string, and UTF-8 token errors agree in both parsers.
#[test]
fn test_json_coordinate_regressions_match_php86_tokens() {
    let source = r#"<?php
$cases = ["{", "{   ", "x", "truex", '{"key": "value', '["é",]', '["€",]', '["😀",]', '{}x', "garbage"];
foreach ($cases as $json) {
    json_decode($json);
    echo json_last_error(), "|", json_last_error_msg(), "\n";
    json_validate($json);
    echo json_last_error(), "|", json_last_error_msg(), "\n";
    try { json_decode($json, null, 512, JSON_THROW_ON_ERROR); }
    catch (JsonException $e) { echo $e->getCode(), "|", $e->getMessage(), "\n"; }
}
json_validate("[[1]]", 1);
echo json_last_error(), "|", json_last_error_msg(), "\n";
json_validate("[[1]]", 2);
echo json_last_error(), "|", json_last_error_msg(), "\n";
"#;
    let mut expected = String::new();
    for column in [2, 5, 1, 5, 9, 6, 6, 6, 3, 1] {
        for _ in 0..3 {
            expected.push_str(&format!("4|Syntax error near location 1:{column}\n"));
        }
    }
    expected.push_str("1|Maximum stack depth exceeded near location 1:1\n");
    expected.push_str("1|Maximum stack depth exceeded near location 1:2\n");
    assert_json_diagnostics_since(source, &expected, 80600);
}
