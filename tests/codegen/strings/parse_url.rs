//! Purpose:
//! End-to-end AOT tests for PHP-compatible `parse_url()` parsing and component selection.
//!
//! Called from:
//! - `cargo test --test codegen_tests_data parse_url` through the strings integration module.
//!
//! Key details:
//! - The array and component corpus is shared with Magician; URL bytes are hex-encoded to preserve control characters.
//! - Tests distinguish missing components from present-empty strings and assert parse-before-selector error ordering.

use crate::support::*;
use serde_json::Value;

/// Verifies the smallest selected-component path completes and returns an owned string.
#[test]
fn test_parse_url_scheme_smoke() {
    let out = compile_and_run("<?php echo parse_url('https://example.com', PHP_URL_SCHEME);");
    assert_eq!(out, "https");
}

/// Runs every PHP-derived shared fixture through the native `parse_url()` array path.
#[test]
fn test_parse_url_shared_fixture_arrays() {
    let cases: Value = serde_json::from_str(include_str!("../../fixtures/parse_url_cases.json"))
        .expect("parse_url fixture JSON must parse");
    let mut source = String::from("<?php\n");
    let mut expected_lines = Vec::new();
    for case in cases.as_array().expect("fixture root must be an array") {
        let url = case["url"].as_str().expect("fixture URL must be a string");
        let url_hex = url
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        source.push_str(&format!(
            "echo json_encode(parse_url(hex2bin(\"{url_hex}\")), JSON_UNESCAPED_SLASHES), \"\\n\";\n"
        ));
        expected_lines.push(parse_url_fixture_json(&case["result"]));
    }
    let actual = compile_and_run(&source);
    assert_eq!(actual, format!("{}\n", expected_lines.join("\n")));
}

/// Runs every shared fixture through all eight PHP component selectors.
#[test]
fn test_parse_url_shared_fixture_components() {
    let cases: Value = serde_json::from_str(include_str!("../../fixtures/parse_url_cases.json"))
        .expect("parse_url fixture JSON must parse");
    let mut source = String::from("<?php\n");
    let mut expected_lines = Vec::new();
    let selectors = [
        "PHP_URL_SCHEME",
        "PHP_URL_HOST",
        "PHP_URL_PORT",
        "PHP_URL_USER",
        "PHP_URL_PASS",
        "PHP_URL_PATH",
        "PHP_URL_QUERY",
        "PHP_URL_FRAGMENT",
    ];
    for case in cases.as_array().expect("fixture root must be an array") {
        let url = case["url"].as_str().expect("fixture URL must be a string");
        let url_hex = url
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let calls = selectors
            .iter()
            .map(|selector| {
                format!(
                    "json_encode(parse_url(hex2bin(\"{url_hex}\"), {selector}), JSON_UNESCAPED_SLASHES)"
                )
            })
            .collect::<Vec<_>>()
            .join(", \"|\", ");
        source.push_str(&format!("echo {calls}, \"\\n\";\n"));
        expected_lines.push(parse_url_fixture_components_json(&case["result"]));
    }
    let actual = compile_and_run(&source);
    assert_eq!(actual, format!("{}\n", expected_lines.join("\n")));
}

/// Verifies invalid-selector ordering for every valid and invalid shared URL fixture.
#[test]
fn test_parse_url_shared_fixture_invalid_selector_ordering() {
    let cases: Value = serde_json::from_str(include_str!("../../fixtures/parse_url_cases.json"))
        .expect("parse_url fixture JSON must parse");
    let mut source = String::from("<?php\n");
    let mut expected_lines = Vec::new();
    for case in cases.as_array().expect("fixture root must be an array") {
        let url = case["url"].as_str().expect("fixture URL must be a string");
        let url_hex = url
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        source.push_str(&format!(
            "try {{ $value = parse_url(hex2bin(\"{url_hex}\"), 8); echo $value === false ? \"false\\n\" : \"unexpected\\n\"; }} catch (\\ValueError $error) {{ echo \"ValueError\\n\"; }}\n"
        ));
        expected_lines.push(if case["result"] == Value::Bool(false) {
            "false"
        } else {
            "ValueError"
        });
    }
    let actual = compile_and_run(&source);
    assert_eq!(actual, format!("{}\n", expected_lines.join("\n")));
}

/// Serializes a fixture result in PHP's stable component insertion order.
fn parse_url_fixture_json(result: &Value) -> String {
    let Some(parts) = result.as_object() else {
        return serde_json::to_string(result).expect("scalar fixture expectation must serialize");
    };
    let entries = [
        "scheme", "host", "port", "user", "pass", "path", "query", "fragment",
    ]
    .into_iter()
    .filter_map(|key| {
        parts.get(key).map(|value| {
            format!(
                "{}:{}",
                serde_json::to_string(key).expect("fixture key must serialize"),
                serde_json::to_string(value).expect("fixture value must serialize")
            )
        })
    })
    .collect::<Vec<_>>();
    format!("{{{}}}", entries.join(","))
}

/// Serializes every component selector expectation from one shared full-result fixture.
fn parse_url_fixture_components_json(result: &Value) -> String {
    let keys = [
        "scheme", "host", "port", "user", "pass", "path", "query", "fragment",
    ];
    let components: Vec<Value> = match result.as_object() {
        Some(parts) => keys
            .into_iter()
            .map(|key| parts.get(key).cloned().unwrap_or(Value::Null))
            .collect(),
        None => vec![Value::Bool(false); keys.len()],
    };
    components
        .into_iter()
        .map(|component| {
            serde_json::to_string(&component)
                .expect("component fixture expectation must serialize")
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Verifies every component selector, missing nulls, empty values, and negative array selectors.
#[test]
fn test_parse_url_component_shapes_and_constants() {
    let out = compile_and_run(
        r#"<?php
$url = "https://user:pass@example.com:8080/path?q=1#frag";
echo parse_url($url, PHP_URL_SCHEME), "|";
echo parse_url($url, PHP_URL_HOST), "|";
echo parse_url($url, PHP_URL_PORT), "|";
echo parse_url($url, PHP_URL_USER), "|";
echo parse_url($url, PHP_URL_PASS), "|";
echo parse_url($url, PHP_URL_PATH), "|";
echo parse_url($url, PHP_URL_QUERY), "|";
echo parse_url($url, PHP_URL_FRAGMENT), "|";
echo parse_url("http://host", PHP_URL_PORT) === null ? "null" : "bad", "|";
echo parse_url("http://host#", PHP_URL_FRAGMENT) === "" ? "empty" : "bad", "|";
echo json_encode(parse_url("/path", -2), JSON_UNESCAPED_SLASHES), "|";
echo defined("PHP_URL_SCHEME") && PHP_URL_FRAGMENT === 7 ? "constants" : "bad";"#,
    );
    assert_eq!(
        out,
        "https|example.com|8080|user|pass|/path|q=1|frag|null|empty|{\"path\":\"/path\"}|constants"
    );
}

/// Verifies named, case-insensitive, namespaced, and first-class callable invocation paths.
#[test]
fn test_parse_url_call_surfaces() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo PaRsE_Url(url: "//named/path", component: \PHP_URL_HOST), "|";
$parse = \parse_url(...);
echo $parse("mailto:a@b", \PHP_URL_PATH), "|";
echo \call_user_func("parse_url", "//callable/path", \PHP_URL_HOST), "|";
echo function_exists("parse_url") ? "exists" : "missing";"#,
    );
    assert_eq!(out, "named|a@b|callable|exists");
}

/// Verifies invalid URLs return false and invalid positive selectors throw the exact catchable ValueError.
#[test]
fn test_parse_url_failure_and_component_value_error() {
    let out = compile_and_run(
        r#"<?php
echo parse_url("http://", 99) === false ? "false" : "bad";
function parse_with_component(int $component): void {
    parse_url("x", $component);
}
try {
    parse_with_component(8);
} catch (\ValueError $error) {
    echo "|", $error->getMessage();
}"#,
    );
    assert_eq!(
        out,
        "false|parse_url(): Argument #2 ($component) must be a valid URL component identifier, 8 given"
    );
}
