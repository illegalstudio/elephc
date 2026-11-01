//! Purpose:
//! End-to-end tests for PHP's `ext/filter` surface: constants, filter_list/id, filter_var,
//! filter_var_array, input helpers, callbacks, options and sanitizers.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected values are independently captured from PHP 8.5.10 (`php -n`) and from the
//!   php-src ext/filter PHPT suite.
//! - Fixtures stay in the normal codegen harness so the bridge is linked and every supported
//!   target runs the same declarations through EIR.

use crate::support::*;

/// Verifies the complete filter and input constant family against PHP 8.5.10.
#[test]
fn test_filter_constant_values_match_php() {
    let out = compile_and_run(
        r#"<?php
echo FILTER_DEFAULT, "|", FILTER_UNSAFE_RAW, "|", FILTER_VALIDATE_INT, "|",
     FILTER_VALIDATE_BOOL, "|", FILTER_VALIDATE_BOOLEAN, "|", FILTER_VALIDATE_FLOAT, "|",
     FILTER_VALIDATE_REGEXP, "|", FILTER_VALIDATE_DOMAIN, "|", FILTER_VALIDATE_URL, "|",
     FILTER_VALIDATE_EMAIL, "|", FILTER_VALIDATE_IP, "|", FILTER_VALIDATE_MAC, "|",
     FILTER_SANITIZE_STRING, "|", FILTER_SANITIZE_ENCODED, "|", FILTER_SANITIZE_SPECIAL_CHARS, "|",
     FILTER_SANITIZE_FULL_SPECIAL_CHARS, "|", FILTER_SANITIZE_EMAIL, "|", FILTER_SANITIZE_URL, "|",
     FILTER_SANITIZE_NUMBER_INT, "|", FILTER_SANITIZE_NUMBER_FLOAT, "|", FILTER_SANITIZE_ADD_SLASHES, "|",
     FILTER_CALLBACK, "|", FILTER_REQUIRE_SCALAR, "|", FILTER_REQUIRE_ARRAY, "|",
     FILTER_FORCE_ARRAY, "|", FILTER_NULL_ON_FAILURE, "|", FILTER_THROW_ON_FAILURE, "|",
     FILTER_FLAG_ALLOW_OCTAL, "|", FILTER_FLAG_ALLOW_HEX, "|", FILTER_FLAG_STRIP_LOW, "|",
     FILTER_FLAG_STRIP_HIGH, "|", FILTER_FLAG_STRIP_BACKTICK, "|", FILTER_FLAG_ENCODE_LOW, "|",
     FILTER_FLAG_ENCODE_HIGH, "|", FILTER_FLAG_ENCODE_AMP, "|", FILTER_FLAG_NO_ENCODE_QUOTES, "|",
     FILTER_FLAG_EMPTY_STRING_NULL, "|", FILTER_FLAG_ALLOW_FRACTION, "|",
     FILTER_FLAG_ALLOW_THOUSAND, "|", FILTER_FLAG_ALLOW_SCIENTIFIC, "|",
     FILTER_FLAG_PATH_REQUIRED, "|", FILTER_FLAG_QUERY_REQUIRED, "|", FILTER_FLAG_IPV4, "|",
     FILTER_FLAG_IPV6, "|", FILTER_FLAG_NO_RES_RANGE, "|", FILTER_FLAG_NO_PRIV_RANGE, "|",
     FILTER_FLAG_GLOBAL_RANGE, "|", FILTER_FLAG_HOSTNAME, "|", FILTER_FLAG_EMAIL_UNICODE, "|",
     INPUT_POST, "|", INPUT_GET, "|", INPUT_COOKIE, "|", INPUT_ENV, "|", INPUT_SERVER;
"#,
    );
    assert_eq!(
        out,
        "516|516|257|258|258|259|272|277|273|274|275|276|513|514|515|522|517|518|519|520|523|1024|33554432|16777216|67108864|134217728|268435456|1|2|4|8|512|16|32|64|128|256|4096|8192|16384|262144|524288|1048576|2097152|4194304|8388608|536870912|1048576|1048576|0|1|2|4|5"
    );
}

/// Verifies filter_list() order and filter_id() case sensitivity.
#[test]
fn test_filter_list_and_id_match_php() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_list());
var_dump(filter_id("stripped"));
var_dump(filter_id("string"));
var_dump(filter_id("url"));
var_dump(filter_id("int"));
var_dump(filter_id("none"));
var_dump(filter_id(-1));
"#,
    );
    let expected = r#"array(21) {
  [0]=>
  string(3) "int"
  [1]=>
  string(7) "boolean"
  [2]=>
  string(5) "float"
  [3]=>
  string(15) "validate_regexp"
  [4]=>
  string(15) "validate_domain"
  [5]=>
  string(12) "validate_url"
  [6]=>
  string(14) "validate_email"
  [7]=>
  string(11) "validate_ip"
  [8]=>
  string(12) "validate_mac"
  [9]=>
  string(6) "string"
  [10]=>
  string(8) "stripped"
  [11]=>
  string(7) "encoded"
  [12]=>
  string(13) "special_chars"
  [13]=>
  string(18) "full_special_chars"
  [14]=>
  string(10) "unsafe_raw"
  [15]=>
  string(5) "email"
  [16]=>
  string(3) "url"
  [17]=>
  string(10) "number_int"
  [18]=>
  string(12) "number_float"
  [19]=>
  string(11) "add_slashes"
  [20]=>
  string(8) "callback"
}
int(513)
int(513)
int(518)
int(257)
bool(false)
bool(false)
"#;
    assert_eq!(out, expected);
}

/// Verifies core FILTER_VALIDATE_INT/FLOAT/BOOL behavior and failure sentinels.
#[test]
fn test_filter_var_core_validation_matches_php() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_var(" 42 ", FILTER_VALIDATE_INT));
var_dump(filter_var("0x1A", FILTER_VALIDATE_INT));
var_dump(filter_var("0x1A", FILTER_VALIDATE_INT, FILTER_FLAG_ALLOW_HEX));
var_dump(filter_var("012", FILTER_VALIDATE_INT, FILTER_FLAG_ALLOW_OCTAL));
var_dump(filter_var("1.234", FILTER_VALIDATE_FLOAT));
var_dump(filter_var("0e5", FILTER_VALIDATE_FLOAT));
var_dump(filter_var("on", FILTER_VALIDATE_BOOL));
var_dump(filter_var("NONE", FILTER_VALIDATE_BOOL));
var_dump(filter_var("NONE", FILTER_VALIDATE_BOOL, FILTER_NULL_ON_FAILURE));
"#,
    );
    assert_eq!(
        out,
        "int(42)\nbool(false)\nint(26)\nint(10)\nfloat(1.234)\nbool(false)\nbool(true)\nbool(false)\nNULL\n"
    );
}

/// Verifies options arrays, min/max and default fallback.
#[test]
fn test_filter_var_options_and_defaults_match_php() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_var("5", FILTER_VALIDATE_INT, ["options" => ["min_range" => 10]]));
var_dump(filter_var("5", FILTER_VALIDATE_INT, ["options" => ["min_range" => 10, "default" => 99]]));
var_dump(filter_var("5", FILTER_VALIDATE_INT, ["flags" => FILTER_NULL_ON_FAILURE, "options" => ["max_range" => 3]]));
var_dump(filter_var("1,234.5", FILTER_VALIDATE_FLOAT, ["flags" => FILTER_FLAG_ALLOW_THOUSAND, "options" => ["thousand" => ","]]));
"#,
    );
    assert_eq!(out, "bool(false)\nint(99)\nNULL\nfloat(1234.5)\n");
}

/// Verifies scalar validation filters and their range flags.
#[test]
fn test_filter_var_network_validation_matches_php() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_var("abuse@example.com", FILTER_VALIDATE_EMAIL));
var_dump(filter_var("invalid", FILTER_VALIDATE_EMAIL));
var_dump(filter_var("http://example.com/index.html", FILTER_VALIDATE_URL));
var_dump(filter_var("http://", FILTER_VALIDATE_URL));
var_dump(filter_var("192.168.0.1", FILTER_VALIDATE_IP));
var_dump(filter_var("192.168.0.1", FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE));
var_dump(filter_var("::1", FILTER_VALIDATE_IP, FILTER_FLAG_IPV6));
var_dump(filter_var("00:11:22:33:44:55", FILTER_VALIDATE_MAC));
var_dump(filter_var("example.com", FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME));
"#,
    );
    assert_eq!(
        out,
        "string(17) \"abuse@example.com\"\nbool(false)\nstring(29) \"http://example.com/index.html\"\nbool(false)\nstring(11) \"192.168.0.1\"\nbool(false)\nstring(3) \"::1\"\nstring(17) \"00:11:22:33:44:55\"\nstring(11) \"example.com\"\n"
    );
}

/// Verifies the sanitizer map and the deprecated string sanitizer.
#[test]
fn test_filter_var_sanitizers_match_php() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_var("qwertyu123456dfghj", FILTER_SANITIZE_NUMBER_INT));
var_dump(filter_var("asd123123.asd123.23", FILTER_SANITIZE_NUMBER_INT));
var_dump(filter_var("\"<br>blah</ph>", FILTER_SANITIZE_ENCODED));
var_dump(filter_var("<>!@#$%^&*()'\"", FILTER_SANITIZE_STRING, FILTER_FLAG_NO_ENCODE_QUOTES));
var_dump(filter_var("<>!@#$%^&*()'\"", FILTER_SANITIZE_STRING, FILTER_FLAG_ENCODE_AMP));
var_dump(filter_var("\xFF", FILTER_SANITIZE_ENCODED));
var_dump(filter_var("test'asd'asd'' asd'\"asdfasdf", FILTER_SANITIZE_ADD_SLASHES));
"#,
    );
    assert_eq!(
        out,
        "string(6) \"123456\"\nstring(11) \"12312312323\"\nstring(26) \"%22%3Cbr%3Eblah%3C%2Fph%3E\"\nstring(12) \"!@#$%^&*()'\"\"\nstring(24) \"!@#$%^&#38;*()&#39;&#34;\"\nstring(3) \"%FF\"\nstring(34) \"test\\'asd\\'asd\\'\\' asd\\'\\\"asdfasdf\"\n"
    );
}

/// Verifies array recursion, REQUIRE_ARRAY and FORCE_ARRAY.
#[test]
fn test_filter_var_arrays_match_php() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_var(["1", "x", ["2"]], FILTER_VALIDATE_INT, FILTER_REQUIRE_ARRAY));
var_dump(filter_var("1", FILTER_VALIDATE_INT, FILTER_FORCE_ARRAY));
var_dump(filter_var(["1"], FILTER_VALIDATE_INT, FILTER_REQUIRE_SCALAR));
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  bool(false)\n  [2]=>\n  array(1) {\n    [0]=>\n    int(2)\n  }\n}\narray(1) {\n  [0]=>\n  int(1)\n}\nbool(false)\n"
    );
}

/// Verifies a simple FILTER_CALLBACK and the invalid-callback TypeError.
#[test]
fn test_filter_var_callback_matches_php() {
    let out = compile_and_run(
        r#"<?php
function upper($value) { return strtoupper($value); }
var_dump(filter_var("data", FILTER_CALLBACK, ["options" => "upper"]));
try { filter_var("qwe", FILTER_CALLBACK, ["options" => "no such func"]); }
catch (TypeError $exception) { echo $exception->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "string(4) \"DATA\"\nfilter_var(): Option must be a valid callback\n"
    );
}

/// Verifies filter_var_array definition arrays, add_empty and unknown-filter warning.
#[test]
fn test_filter_var_array_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$data = ["test" => "42"];
var_dump(filter_var_array($data, ["test" => FILTER_VALIDATE_INT]));
var_dump(filter_var_array($data, ["test" => ["filter" => FILTER_VALIDATE_INT]]));
var_dump(filter_var_array($data, ["missing" => FILTER_VALIDATE_INT]));
var_dump(filter_var_array($data, ["missing" => FILTER_VALIDATE_INT], false));
var_dump(filter_var_array($data, ["test" => -1]));
"#,
    );
    assert_eq!(
        out.stdout,
        "array(1) {\n  [\"test\"]=>\n  int(42)\n}\narray(1) {\n  [\"test\"]=>\n  int(42)\n}\narray(1) {\n  [\"missing\"]=>\n  NULL\n}\narray(0) {\n}\narray(1) {\n  [\"test\"]=>\n  string(2) \"42\"\n}\n"
    );
    assert!(out.stderr.contains("filter_var_array(): Unknown filter with ID -1"));
}



/// Verifies filter_input/filter_has_var/filter_input_array over the request superglobals.
#[test]
fn test_filter_input_surface_matches_php_request_storage() {
    let out = compile_and_run(
        r#"<?php
__elephc_filter_seed_request(INPUT_GET, ["foo" => "<b>abc</b>", "a" => "1"], "unsafe_raw", 0, false);
__elephc_filter_seed_request(INPUT_POST, ["d" => "12345.7"], "unsafe_raw", 0, false);
var_dump(filter_has_var(INPUT_GET, "foo"));
var_dump(filter_has_var(INPUT_GET, "missing"));
var_dump(filter_input(INPUT_GET, "foo", FILTER_SANITIZE_SPECIAL_CHARS));
var_dump(filter_input(INPUT_GET, "a", FILTER_VALIDATE_INT));
var_dump(filter_input(INPUT_POST, "d", FILTER_VALIDATE_FLOAT));
var_dump(filter_input(INPUT_GET, "missing"));
var_dump(filter_input(INPUT_GET, "missing", FILTER_VALIDATE_INT, FILTER_NULL_ON_FAILURE));
var_dump(filter_input_array(INPUT_GET, ["foo" => FILTER_SANITIZE_SPECIAL_CHARS]));
var_dump(filter_input_array(INPUT_GET, ["missing" => FILTER_VALIDATE_INT]));
var_dump(filter_input_array(INPUT_GET));
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nstring(26) \"&#60;b&#62;abc&#60;/b&#62;\"\nint(1)\nfloat(12345.7)\nNULL\nbool(false)\narray(1) {\n  [\"foo\"]=>\n  string(26) \"&#60;b&#62;abc&#60;/b&#62;\"\n}\narray(1) {\n  [\"missing\"]=>\n  NULL\n}\narray(2) {\n  [\"foo\"]=>\n  string(10) \"<b>abc</b>\"\n  [\"a\"]=>\n  string(1) \"1\"\n}\n"
    );
}

/// Verifies FILTER_THROW_ON_FAILURE and the Filter exception hierarchy.
#[test]
fn test_filter_throw_on_failure_matches_php() {
    let out = compile_and_run(
        r#"<?php
try { filter_var("invalid", FILTER_VALIDATE_INT, FILTER_THROW_ON_FAILURE); }
catch (\Filter\FilterFailedException $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { filter_var([], FILTER_VALIDATE_INT, FILTER_THROW_ON_FAILURE); }
catch (\Filter\FilterFailedException $e) { echo $e->getMessage(), "\n"; }
var_dump(filter_var("42", FILTER_VALIDATE_INT, FILTER_THROW_ON_FAILURE));
"#,
    );
    assert_eq!(
        out,
        "Filter\\FilterFailedException: filter validation failed: filter int not satisfied by 'invalid'\nfilter validation failed: not a scalar value (got an array)\nint(42)\n"
    );
}








/// Verifies loading the filter surface with the default `filter.default` leaves CLI superglobals
/// untouched, including `$_ENV` populated by the CLI SAPI.
#[test]
fn test_filter_surface_preserves_cli_superglobals_without_filter_default() {
    let out = compile_and_run(
        r#"<?php
$before = count($_ENV);
filter_var("x", FILTER_SANITIZE_SPECIAL_CHARS);
echo $before > 0 && count($_ENV) === $before ? "kept" : "lost";
"#,
    );
    assert_eq!(out, "kept");
}

/// Unknown filter warnings use PHP's engine `E_WARNING` level (2), not `E_USER_WARNING`.
#[test]
fn test_filter_unknown_filter_warning_uses_engine_level() {
    let output = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $errno): bool { echo "errno=", $errno; return true; });
filter_var("x", 999);
"#,
    );
    assert_eq!(output.stdout.trim(), "errno=2");
}

/// PCRE spelling errors warn at engine level and fail validation instead of matching.
#[test]
fn test_filter_regexp_unknown_modifier_warns_and_fails() {
    let output = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $errno, "|", $message, "\n";
    return true;
});
var_dump(filter_var("foo/bar/", FILTER_VALIDATE_REGEXP, ["options" => ["regexp" => "/foo/bar/"]]));
"#,
    );
    assert_eq!(
        output.stdout,
        "2|filter_var(): Unknown modifier 'b'\nbool(false)\n"
    );
}

/// Invalid regexp bodies report PCRE2's exact compilation diagnostic and offset.
#[test]
fn test_filter_regexp_compile_errors_match_pcre2() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $errno, "|", $message, "\n";
    return true;
});
$patterns = [
    "/[/",
    "/(/",
    "/a{2,1}/",
    "/(?<=a+)/",
    "/[z-a]/",
    "/\\x{110000}/u",
    "/(?<x>a)(?<x>b)/",
];
foreach ($patterns as $pattern) {
    var_dump(filter_var("a", FILTER_VALIDATE_REGEXP, ["options" => ["regexp" => $pattern]]));
}
"#,
    );
    // The managed PCRE2 10.47 messages match PHP exactly. The host provider used by the
    // test harness can be older on Linux, so the two version-sensitive diagnostics accept
    // each PCRE2 revision's wording while the stable cases stay exact.
    for expected in [
        "2|filter_var(): Compilation failed: missing terminating ] for character class at offset 1\n",
        "2|filter_var(): Compilation failed: missing closing parenthesis at offset 1\n",
        "2|filter_var(): Compilation failed: numbers out of order in {} quantifier at offset 5\n",
        "2|filter_var(): Compilation failed: character code point value in \\x{} or \\o{} is too large at offset 9\n",
        "2|filter_var(): Compilation failed: two named subpatterns have the same name (PCRE2_DUPNAMES not set) at offset 12\n",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in {out:?}");
    }
    assert!(
        out.contains("lookbehind assertion is not fixed length at offset 0")
            || out.contains("length of lookbehind assertion is not limited at offset 0"),
        "lookbehind diagnostic: {out:?}"
    );
    assert!(
        out.contains("range out of order in character class at offset 3")
            || out.contains("range out of order in character class at offset 4"),
        "range diagnostic: {out:?}"
    );
    assert_eq!(out.matches("bool(false)").count(), 7, "{out:?}");
}

/// Verifies the real PHP 8.5 URI parser class names and WhatWg backslash-host handling.
#[test]
fn test_filter_url_uri_parser_classes_match_php85() {
    let out = compile_and_run(
        r#"<?php
$url = "http://example.com\\path";
var_dump(filter_var($url, FILTER_VALIDATE_URL, ["options" => ["uri_parser_class" => "Uri\\WhatWg\\Url"]]));
var_dump(filter_var($url, FILTER_VALIDATE_URL, ["options" => ["uri_parser_class" => "Uri\\Rfc3986\\Uri"]]));
try {
    filter_var($url, FILTER_VALIDATE_URL, ["options" => ["uri_parser_class" => "Uri\\WhatWgUri"]]);
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "string(23) \"http://example.com\\path\"\nbool(false)\nfilter_var(): \"uri_parser_class\" option has invalid value\n"
    );
}

/// `Uri\WhatWg\Url` uses the WHATWG parser, including its port, IPv4 and backslash rules.
#[test]
fn test_filter_url_whatwg_parser_matches_php() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    ["http://example.com", 0],
    ["http://example.com\\path", 0],
    ["http://example.com:65535", 0],
    ["http://example.com:65536", 0],
    ["http://0x7f.1", 0],
    ["http://999.999.999.999", 0],
    ["http://[::1]", 0],
    ["foo://bar", 0],
    ["foo:bar", 0],
    ["mailto:", 0],
    ["file:///tmp/x", 0],
    ["http://example.com", FILTER_FLAG_QUERY_REQUIRED],
    ["http://example.com?x", FILTER_FLAG_QUERY_REQUIRED],
];
foreach ($cases as $i => $case) {
    $result = filter_var($case[0], FILTER_VALIDATE_URL, [
        "options" => ["uri_parser_class" => "Uri\\WhatWg\\Url"],
        "flags" => $case[1],
    ]);
    echo $i, ":", $result === false ? "false" : $result, "\n";
}
"#,
    );
    assert_eq!(
        out,
        "0:http://example.com\n\
1:http://example.com\\path\n\
2:http://example.com:65535\n\
3:false\n\
4:http://0x7f.1\n\
5:false\n\
6:http://[::1]\n\
7:foo://bar\n\
8:false\n\
9:mailto:\n\
10:file:///tmp/x\n\
11:false\n\
12:http://example.com?x\n"
    );
}

/// The filter.default deprecation reports PHP's INI-startup file/line pair.
#[test]
fn test_filter_default_deprecation_uses_ini_startup_location() {
    let output = compile_and_run_capture(
        r#"<?php
__elephc_filter_seed_request(INPUT_GET, [], "special_chars", 0, true);
echo "Done\n";
"#,
    );
    assert_eq!(output.stdout, "Done\n");
    assert!(
        output.stderr.contains("The filter.default ini setting is deprecated in Unknown on line 0"),
        "stderr: {:?}",
        output.stderr
    );
}

/// A strict included file keeps its strict filter checks when the main file is weak.
#[test]
fn test_filter_strict_types_survive_includes() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require __DIR__ . '/strict.php';
try {
    strict_filter("1");
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
            ),
            (
                "strict.php",
                r#"<?php
declare(strict_types=1);
function strict_filter(mixed $filter): mixed {
    return filter_var("x", $filter);
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "filter_var(): Argument #2 ($filter) must be of type int, string given\n"
    );
}

/// CLI filter storage exposes only PHP's five SAPI-owned server keys.
#[test]
fn test_filter_input_cli_server_storage_uses_sapi_keys() {
    let out = compile_and_run(
        r#"<?php
var_dump(filter_input(INPUT_SERVER, "PATH"));
var_dump(filter_input(INPUT_SERVER, "PHP_SELF") !== null);
var_dump(filter_has_var(INPUT_SERVER, "HOME"));
"#,
    );
    assert_eq!(out, "NULL\nbool(true)\nbool(false)\n");
}

/// `filter_input_array(INPUT_ENV)` participates in the CLI raw snapshot trigger.
#[test]
fn test_filter_input_array_env_seeds_cli_raw_storage() {
    let out = compile_and_run_capture_with_ini(
        r#"<?php var_dump(is_array(filter_input_array(INPUT_ENV)));"#,
        &[("variables_order".to_string(), "GPCSE".to_string())],
    );
    assert_eq!(out.stdout.trim(), "bool(true)");
}

/// Filter integer argument coercion emits PHP's weak-mode deprecations and range errors.
#[test]
fn test_filter_integer_argument_coercion_matches_php() {
    let output = compile_and_run_capture(
        r#"<?php
set_error_handler(function (int $errno, string $message): bool {
    echo $errno, "|", $message, "\n";
    return true;
});
filter_var("x", null);
filter_var("x", 1.5);
filter_var("x", "1.5");
filter_var("x", 1.0);
filter_var("x", "1.0");
filter_var("x", "1e2");
filter_var("x", "1e-2");
filter_var("x", " 1.5 ");
filter_var("x", "1.");
filter_var("x", "1");
echo "options:";
var_dump(filter_var("1", FILTER_VALIDATE_INT, 1.5));
echo "options:";
var_dump(filter_var("1", FILTER_VALIDATE_INT, "1.5"));
echo "options:";
var_dump(filter_var("1", FILTER_VALIDATE_INT, null));
echo "options:";
var_dump(filter_var("1", FILTER_VALIDATE_INT, true));
try {
    filter_var("x", "9223372036854775808");
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    filter_var("1", FILTER_VALIDATE_INT, "9223372036854775808");
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    filter_var("x", NAN);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
try {
    filter_var("x", INF);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        output.stdout,
        r#"8192|filter_var(): Passing null to parameter #2 ($filter) of type int is deprecated
2|filter_var(): Unknown filter with ID 0
8192|Implicit conversion from float 1.5 to int loses precision
2|filter_var(): Unknown filter with ID 1
8192|Implicit conversion from float-string "1.5" to int loses precision
2|filter_var(): Unknown filter with ID 1
2|filter_var(): Unknown filter with ID 1
2|filter_var(): Unknown filter with ID 1
2|filter_var(): Unknown filter with ID 100
8192|Implicit conversion from float-string "1e-2" to int loses precision
2|filter_var(): Unknown filter with ID 0
8192|Implicit conversion from float-string " 1.5 " to int loses precision
2|filter_var(): Unknown filter with ID 1
2|filter_var(): Unknown filter with ID 1
2|filter_var(): Unknown filter with ID 1
options:8192|Implicit conversion from float 1.5 to int loses precision
int(1)
options:8192|Implicit conversion from float-string "1.5" to int loses precision
int(1)
options:8192|filter_var(): Passing null to parameter #3 ($options) of type array|int is deprecated
int(1)
options:int(1)
filter_var(): Argument #2 ($filter) must be of type int, string given
filter_var(): Argument #3 ($options) must be of type array|int, string given
filter_var(): Argument #2 ($filter) must be of type int, float given
filter_var(): Argument #2 ($filter) must be of type int, float given
"#
    );
}

/// PHP's full-special-chars sanitizer preserves only valid HTML 4.01 and numeric entities.
#[test]
fn test_filter_full_special_chars_preserves_entities() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "&#x41;",
    "&#65;a&#39;",
    "&#0;",
    "&#x;",
    "&#999999999999999999999;",
    "&#xFFFFFFFF;",
    "&#x10FFFF;",
    "&#x110000;",
    "&#12a;",
    "&#x41g;",
    "&#000000000000000000000000000000000000000000000000000000000000041;",
    "&#xD800;",
    "&amp;",
    "&nbsp;",
    "&copy;",
    "&notin;",
    "&Aacute;",
    "&sum;",
    "&Sigma;",
    "&&amp;",
    "&AMP;",
    "&apos;",
    "&#+65;",
    "&#-1;",
];
foreach ($cases as $case) {
    echo filter_var($case, FILTER_SANITIZE_FULL_SPECIAL_CHARS), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "&#x41;\n&#65;a&#39;\n&#0;\n&amp;#x;\n&amp;#999999999999999999999;\n&amp;#xFFFFFFFF;\n&#x10FFFF;\n&amp;#x110000;\n&amp;#12a;\n&amp;#x41g;\n&#000000000000000000000000000000000000000000000000000000000000041;\n&#xD800;\n&amp;\n&nbsp;\n&copy;\n&notin;\n&Aacute;\n&sum;\n&Sigma;\n&amp;&amp;\n&amp;AMP;\n&amp;apos;\n&amp;#+65;\n&amp;#-1;\n"
    );
}

/// A strict_types filter call validates the PHP signature before weak coercion.
#[test]
fn test_filter_strict_types_scalar_arguments() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
$cases = [
    fn() => filter_var("x", "1"),
    fn() => filter_var("x", 1.0),
    fn() => filter_var("x", FILTER_VALIDATE_INT, "1"),
    fn() => filter_var("x", FILTER_VALIDATE_INT, null),
    fn() => filter_var(value: "x", filter: "1"),
    fn() => filter_var_array("x", FILTER_VALIDATE_INT),
    fn() => filter_var_array([], FILTER_VALIDATE_INT, 1),
    fn() => filter_input("GET", "x"),
    fn() => filter_input(INPUT_GET, 1),
    fn() => filter_input_array("GET"),
    fn() => filter_input_array(INPUT_GET, FILTER_VALIDATE_INT, 1),
    fn() => filter_has_var("GET", "x"),
    fn() => filter_has_var(INPUT_GET, 1),
    fn() => filter_id(1),
    function () { $filter = "1"; return filter_var("x", $filter); },
    function () { return call_filter("1"); },
];
function call_filter(mixed $filter) {
    return filter_var("x", $filter);
}
foreach ($cases as $case) {
    try {
        var_dump($case());
    } catch (Throwable $error) {
        echo get_class($error), ": ", $error->getMessage(), "\n";
    }
}
var_dump(filter_var("42", FILTER_VALIDATE_INT));
var_dump(filter_var("x", FILTER_VALIDATE_INT, []));
var_dump(filter_id("int"));
"#,
    );
    assert_eq!(
        out,
        "TypeError: filter_var(): Argument #2 ($filter) must be of type int, string given\n\
TypeError: filter_var(): Argument #2 ($filter) must be of type int, float given\n\
TypeError: filter_var(): Argument #3 ($options) must be of type array|int, string given\n\
TypeError: filter_var(): Argument #3 ($options) must be of type array|int, null given\n\
TypeError: filter_var(): Argument #2 ($filter) must be of type int, string given\n\
TypeError: filter_var_array(): Argument #1 ($array) must be of type array, string given\n\
TypeError: filter_var_array(): Argument #3 ($add_empty) must be of type bool, int given\n\
TypeError: filter_input(): Argument #1 ($type) must be of type int, string given\n\
TypeError: filter_input(): Argument #2 ($var_name) must be of type string, int given\n\
TypeError: filter_input_array(): Argument #1 ($type) must be of type int, string given\n\
TypeError: filter_input_array(): Argument #3 ($add_empty) must be of type bool, int given\n\
TypeError: filter_has_var(): Argument #1 ($input_type) must be of type int, string given\n\
TypeError: filter_has_var(): Argument #2 ($var_name) must be of type string, int given\n\
TypeError: filter_id(): Argument #1 ($name) must be of type string, int given\n\
TypeError: filter_var(): Argument #2 ($filter) must be of type int, string given\n\
TypeError: filter_var(): Argument #2 ($filter) must be of type int, string given\n\
int(42)\n\
bool(false)\n\
int(257)\n"
    );
}

/// Strictness follows the call site, so a weak included file still coerces filter arguments.
#[test]
fn test_filter_strict_types_follow_the_call_site_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
declare(strict_types=1);
require __DIR__ . '/weak.php';
var_dump(weak_filter("1"));
try {
    filter_var("x", "1");
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
"#,
            ),
            (
                "weak.php",
                r#"<?php
function weak_filter(mixed $filter): mixed {
    return @filter_var("x", $filter);
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "bool(false)\nfilter_var(): Argument #2 ($filter) must be of type int, string given\n"
    );
}

/// Verifies function_exists, extension_loaded and literal eval see the filter surface.
#[test]
fn test_filter_surface_visibility_and_eval() {
    let out = compile_and_run(
        r#"<?php
var_dump(function_exists("filter_var"));
var_dump(function_exists("FILTER_VALIDATE_INT"));
var_dump(extension_loaded("filter"));
var_dump(eval('return filter_var("42", FILTER_VALIDATE_INT);'));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nint(42)\n");
}

/// `filter.default=int` keeps the filter's native result in the request superglobal.
#[test]
fn test_filter_default_keeps_native_request_value_types() {
    let out = compile_and_run(
        r#"<?php
__elephc_filter_seed_request(INPUT_GET, ["ok" => "12", "bad" => "nope"], "int", 0, false);
var_dump($_GET["ok"], $_GET["bad"]);
"#,
    );
    assert!(out.contains("int(12)"), "stdout: {out:?}");
    assert!(out.contains("bool(false)"), "stdout: {out:?}");
    assert!(!out.contains("string(2) \"12\""), "stdout: {out:?}");
    assert!(!out.contains("string(0) \"\""), "stdout: {out:?}");
}


/// The codegen fixture harness feeds its INI overrides into the shared startup configuration,
/// so `ini_get()` reports the same directives the PHPT `--INI--` section declared.
#[test]
fn test_filter_default_ini_get_uses_fixture_overrides() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php echo ini_get("filter.default"), "|", ini_get("filter.default_flags");"#,
        &[
            ("filter.default".to_string(), "special_chars".to_string()),
            ("filter.default_flags".to_string(), "4".to_string()),
        ],
    );
    assert_eq!(output.stdout, "special_chars|4");
}
/// `filter_var_array()` with a scalar filter writes through referenced input elements, exactly
/// like php-src's duplicate-then-filter-in-place behaviour.
#[test]
fn test_filter_var_array_scalar_preserves_reference_entries() {
    let out = compile_and_run(
        r#"<?php
$array = ["123foo"];
$array2 = [&$array];
$filtered = filter_var_array($array2, FILTER_VALIDATE_INT);
var_dump($filtered);
var_dump($array);
"#,
    );
    assert_eq!(
        out,
        "array(1) {\n  [0]=>\n  &array(1) {\n    [0]=>\n    bool(false)\n  }\n}\narray(1) {\n  [0]=>\n  bool(false)\n}\n"
    );
}
