//! Purpose:
//! Exercises the `parse_str` prelude declaration and its `filter.default` post-pass.
//!
//! Called from:
//! - `cargo test` through the strings integration suite.
//!
//! Key details:
//! - Parsing reuses the native `mb_parse_str` query engine, including bracketed keys.
//! - The untyped by-reference result parameter must be populated through the caller's variable.
//! - A configured `filter.default` filters every parsed leaf and turns rejected values into `""`.

use crate::support::*;

/// Parses flat keys, indexed lists, and named nested keys into the caller's variable.
#[test]
fn test_parse_str_populates_nested_output() {
    let out = compile_and_run(
        r#"<?php
parse_str("a=1&b[]=2&b[]=3&c[x]=4", $out);
echo $out["a"], "|", $out["b"][0], "|", $out["b"][1], "|", $out["c"]["x"];
"#,
    );
    assert_eq!(out, "1|2|3|4");
}

/// Named arguments bind to `string` and the by-reference `result` parameter.
#[test]
fn test_parse_str_named_arguments() {
    let out = compile_and_run(
        r#"<?php
parse_str(result: $out, string: "a=1&b=2");
echo $out["a"], "|", $out["b"];
"#,
    );
    assert_eq!(out, "1|2");
}

/// `zend_try_array_init()` replaces any previous output value, so initialized scalars,
/// strings, and objects become the parsed array just like an unset or null variable.
#[test]
fn test_parse_str_replaces_non_array_outputs() {
    let out = compile_and_run(
        r#"<?php
$int = 42;
parse_str("a=1", $int);
$string = "old";
parse_str("b=2", $string);
$object = new stdClass();
parse_str("c=3", $object);
echo json_encode($int), "|", json_encode($string), "|", json_encode($object);
"#,
    );
    assert_eq!(out, "{\"a\":\"1\"}|{\"b\":\"2\"}|{\"c\":\"3\"}");
}

/// Named arguments are evaluated in source order: a later argument still reads the output's
/// previous value, and the output is replaced only once every argument has been evaluated.
#[test]
fn test_parse_str_named_output_keeps_later_arguments() {
    let out = compile_and_run(
        r#"<?php
$out = "a=1";
parse_str(result: $out, string: $out);
echo json_encode($out), "
";
$out = "a=1";
parse_str(result: $out, string: ($out = "b=2"));
echo json_encode($out);
"#,
    );
    assert_eq!(out, "{\"a\":\"1\"}
{\"b\":\"2\"}");
}

/// `parse_str()` preserves raw query bytes independently of the mbstring input charset.
#[test]
fn test_parse_str_preserves_raw_query_bytes() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
parse_str("a=%E9&b=caf%E9", $out);
echo bin2hex($out["a"]), "|", bin2hex($out["b"]);
"#,
        &[
            ("mbstring.http_input".to_string(), "ISO-8859-1".to_string()),
            ("mbstring.internal_encoding".to_string(), "UTF-8".to_string()),
            ("mbstring.encoding_translation".to_string(), "0".to_string()),
        ],
    );
    assert_eq!(output.stdout, "e9|636166e9");
}

/// `filter.default=int` rejects `b=notint` into the empty string, matching PHP's input filter.
#[test]
fn test_parse_str_applies_filter_default_and_rejects_invalid_values() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
parse_str("a=1&b=notint", $out);
var_dump($out);
"#,
        &[("filter.default".to_string(), "int".to_string())],
    );
    assert!(output.stdout.contains("string(1) \"1\""), "stdout: {:?}", output.stdout);
    assert!(output.stdout.contains("string(0) \"\""), "stdout: {:?}", output.stdout);
    assert!(!output.stdout.contains("bool(false)"), "stdout: {:?}", output.stdout);
}

/// `max_input_vars` keeps the fields PHP allows and warns after registering them; the shared
/// `mb_parse_str()` path keeps its whole-query rejection.
#[test]
fn test_parse_str_keeps_fields_allowed_by_max_input_vars() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $errno, "|", $message, "
";
    return true;
});
$flat = [];
parse_str("a=1&b=2&c=3", $flat);
echo $flat["a"], "|", $flat["b"], "|", isset($flat["c"]) ? "c" : "no-c", "
";
$nested = [];
parse_str("a[x]=1&a[y]=2&b=3", $nested);
echo $nested["a"]["x"], "|", $nested["a"]["y"], "|", isset($nested["b"]) ? "b" : "no-b", "
";
$mb = ["kept" => "old"];
var_dump(mb_parse_str("a=1&b=2&c=3", $mb));
var_dump($mb);
"#,
        &[("max_input_vars".to_string(), "2".to_string())],
    );
    assert_eq!(
        output.stdout,
        "2|parse_str(): Input variables exceeded 2. To increase the limit change max_input_vars in php.ini.\n1|2|no-c
2|parse_str(): Input variables exceeded 2. To increase the limit change max_input_vars in php.ini.\n1|2|no-b
2|mb_parse_str(): Input variables exceeded 2. To increase the limit change max_input_vars in php.ini.\nbool(false)
array(0) {
}
",
        "stderr: {}",
        output.stderr
    );
}

/// The `max_input_vars` warning is emitted after the accepted prefix is published, so an
/// error handler sees those fields and a throwing handler leaves them in the caller's variable.
#[test]
fn test_parse_str_publishes_allowed_fields_before_the_limit_warning() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
$out = ["old" => "keep"];
set_error_handler(function (int $errno, string $message) use (&$out): bool {
    echo json_encode($out), "\n";
    throw new RuntimeException("stop");
});
try { parse_str("a=1&b=2", $out); } catch (Throwable $error) { echo "caught\n"; }
echo json_encode($out), "\n";
"#,
        &[("max_input_vars".to_string(), "1".to_string())],
    );
    assert_eq!(
        output.stdout, "{\"a\":\"1\"}\ncaught\n{\"a\":\"1\"}\n",
        "stderr: {}",
        output.stderr
    );
}

/// A zero `max_input_vars` keeps no fields and still warns with PHP's "exceeded 0" text.
#[test]
fn test_parse_str_zero_max_input_vars_warns() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $errno, "|", $message, "
";
    return true;
});
$out = ["old" => "keep"];
parse_str("a=1", $out);
var_dump($out);
"#,
        &[("max_input_vars".to_string(), "0".to_string())],
    );
    assert_eq!(
        output.stdout,
        "2|parse_str(): Input variables exceeded 0. To increase the limit change max_input_vars in php.ini.
array(0) {
}
",
        "stderr: {}",
        output.stderr
    );
}

/// `parse_str()` is byte-preserving: it must not run mbstring input detection, warn, or
/// overwrite the state `mb_http_input()` reads.
#[test]
fn test_parse_str_skips_mbstring_input_detection() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $errno, "|", $message, "
";
    return true;
});
var_dump(mb_http_input("G"));
parse_str("a=%E9", $out);
echo bin2hex($out["a"]), "
";
var_dump(mb_http_input("G"));
"#,
        &[
            ("mbstring.http_input".to_string(), "UTF-8,ASCII".to_string()),
            ("mbstring.strict_detection".to_string(), "1".to_string()),
            ("mbstring.encoding_translation".to_string(), "0".to_string()),
        ],
    );
    assert_eq!(
        output.stdout, "bool(false)\ne9\nbool(false)\n",
        "stderr: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("Unable to detect encoding"),
        "stderr: {}",
        output.stderr
    );
}

/// A configured sanitizer filters nested values recursively.
#[test]
fn test_parse_str_filter_default_walks_nested_arrays() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
parse_str("a=%3Cb%3E&list[]=%3Ci%3E", $out);
echo $out["a"], "|", $out["list"][0];
"#,
        &[("filter.default".to_string(), "special_chars".to_string())],
    );
    assert_eq!(output.stdout, "&#60;b&#62;|&#60;i&#62;");
}
