//! Purpose:
//! Checks shared mbstring scalar contracts for invalid arity and argument types.
//!
//! Called from:
//! - The compiler diagnostic integration test binary.
//!
//! Key details:
//! - Nullable integer and string parameters retain distinct diagnostic types.

use super::*;

/// Rejects impossible callback-replacement argument counts and outer scalar types.
#[test]
fn test_error_mbstring_regex_callback_contract() {
    for (source, message) in [
        ("<?php mb_ereg_replace_callback();", "mb_ereg_replace_callback() takes 3 or 4 arguments"),
        ("<?php mb_ereg_replace_callback(\"a\", \"count\");", "mb_ereg_replace_callback() takes 3 or 4 arguments"),
        ("<?php mb_ereg_replace_callback([], \"count\", \"a\");", "mb_ereg_replace_callback() pattern argument must be string"),
        ("<?php mb_ereg_replace_callback(\"a\", 1, \"a\");", "mb_ereg_replace_callback() callback argument must be callable"),
        ("<?php mb_ereg_replace_callback(\"a\", \"count\", []);", "mb_ereg_replace_callback() string argument must be string"),
        ("<?php mb_ereg_replace_callback(\"a\", \"count\", \"a\", []);", "mb_ereg_replace_callback() options argument must be string or null"),
    ] { expect_error(source, message); }
}

/// Accepts a known capture callback without treating its signature as an immediate invocation.
#[test]
fn test_mbstring_regex_callback_context_defers_invocation_validation() {
    check_source(
        "<?php function copy_capture(array $m, string $unused): string { return $m[0]; } \
         mb_ereg_replace_callback(\"z\", \"copy_capture\", \"abc\");",
    )
    .expect("a callback with no matching invocation must pass frontend validation");
}

/// Rejects wrong output-handler arity and impossible scalar types using the shared public contract.
#[test]
fn test_error_mbstring_output_handler_contract() {
    for (source, message) in [
        ("<?php mb_output_handler();", "mb_output_handler() takes exactly 2 arguments"),
        ("<?php mb_output_handler(\"a\");", "mb_output_handler() takes exactly 2 arguments"),
        ("<?php mb_output_handler(\"a\", 9, 1);", "mb_output_handler() takes exactly 2 arguments"),
        ("<?php mb_output_handler([], 9);", "mb_output_handler() string argument must be string"),
        ("<?php mb_output_handler(\"a\", []);", "mb_output_handler() status argument must be int"),
        ("<?php declare(strict_types=1); mb_output_handler(1, 9);", "mb_output_handler() string argument must be string"),
        ("<?php declare(strict_types=1); mb_output_handler(\"a\", \"9\");", "mb_output_handler() status argument must be int"),
    ] { expect_error(source, message); }
}

/// Rejects invalid query arity, impossible source types, and non-writable output expressions.
#[test]
fn test_error_mbstring_parse_str_contract() {
    for (source, message) in [
        ("<?php mb_parse_str();", "mb_parse_str() takes exactly 2 arguments"),
        ("<?php mb_parse_str(\"a=b\");", "mb_parse_str() takes exactly 2 arguments"),
        ("<?php mb_parse_str([], $result);", "mb_parse_str() string argument must be string"),
        ("<?php declare(strict_types=1); mb_parse_str(1, $result);", "mb_parse_str() string argument must be string"),
        ("<?php mb_parse_str(\"a=b\", null);", "mb_parse_str(): Argument #2 ($result) could not be passed by reference"),
    ] { expect_error(source, message); }
}

/// Rejects invalid capture arity and strict input shapes using the shared function contract.
#[test]
fn test_error_mbstring_regex_capture_contracts() {
    for (source, message) in [
        ("<?php mb_ereg();", "mb_ereg() takes 2 or 3 arguments"),
        ("<?php mb_eregi(\"a\");", "mb_eregi() takes 2 or 3 arguments"),
        ("<?php mb_ereg([], \"a\");", "mb_ereg() pattern argument must be string"),
        ("<?php mb_eregi(\"a\", []);", "mb_eregi() string argument must be string"),
        ("<?php declare(strict_types=1); mb_ereg(1, \"a\");", "mb_ereg() pattern argument must be string"),
        ("<?php mb_ereg(\"a\", \"a\", null);", "mb_ereg(): Argument #3 ($matches) could not be passed by reference"),
        ("<?php mb_eregi(matches: [], pattern: \"a\", string: \"a\");", "mb_eregi(): Argument #3 ($matches) could not be passed by reference"),
    ] { expect_error(source, message); }
}

/// Rejects invalid static arity and strict argument types for both shared mbregex settings.
#[test]
fn test_error_mbstring_regex_settings_contracts() {
    for (source, message) in [
        ("<?php mb_regex_encoding(null, null);", "mb_regex_encoding() takes at most 1 argument"),
        ("<?php mb_regex_set_options(null, null);", "mb_regex_set_options() takes at most 1 argument"),
        ("<?php mb_regex_encoding([]);", "mb_regex_encoding() encoding argument must be string or null"),
        ("<?php mb_regex_set_options([]);", "mb_regex_set_options() options argument must be string or null"),
        ("<?php declare(strict_types=1); mb_regex_encoding(1);", "mb_regex_encoding() encoding argument must be string or null"),
        ("<?php declare(strict_types=1); mb_regex_set_options(false);", "mb_regex_set_options() options argument must be string or null"),
        ("<?php function encoding(): string { return mb_regex_encoding(); }", "Function 'encoding' return type expects Str, got Union([Str, Bool])"),
    ] { expect_error(source, message); }
}

/// Rejects impossible information selector types and excessive arguments before lowering.
#[test]
fn test_error_mbstring_info_contract() {
    for (source, message) in [
        ("<?php mb_http_input(\"G\", \"P\");", "mb_http_input() takes at most 1 argument"),
        ("<?php mb_http_input([]);", "mb_http_input() type argument must be string or null"),
        ("<?php declare(strict_types=1); mb_http_input(false);", "mb_http_input() type argument must be string or null"),
        ("<?php mb_get_info(\"all\", \"language\");", "mb_get_info() takes at most 1 argument"),
        ("<?php mb_get_info([]);", "mb_get_info() type argument must be string"),
        ("<?php declare(strict_types=1); mb_get_info(null);", "mb_get_info() type argument must be string"),
    ] { expect_error(source, message); }
}

/// Rejects invalid static MIME calls before runtime argument coercion.
#[test]
fn test_error_mbstring_mime_contract() {
    for (source, message) in [
        ("<?php mb_encode_mimeheader();", "mb_encode_mimeheader() takes 1 to 5 arguments"),
        ("<?php mb_encode_mimeheader(1, 2, 3, 4, 5, 6);", "mb_encode_mimeheader() takes 1 to 5 arguments"),
        ("<?php mb_encode_mimeheader([]);", "mb_encode_mimeheader() string argument must be string"),
        ("<?php mb_encode_mimeheader(\"x\", []);", "mb_encode_mimeheader() charset argument must be string or null"),
        ("<?php mb_decode_mimeheader();", "mb_decode_mimeheader() takes exactly 1 argument"),
        ("<?php mb_decode_mimeheader([], []);", "mb_decode_mimeheader() takes exactly 1 argument"),
        ("<?php mb_decode_mimeheader([]);", "mb_decode_mimeheader() string argument must be string"),
    ] {
        expect_error(source, message);
    }
}

/// Verifies every added scalar operation rejects invalid counts and non-scalar inputs.
#[test]
fn test_error_mbstring_scalar_contracts() {
    for (source, message) in [
        ("<?php mb_substr();", "mb_substr() takes 2 to 4 arguments"),
        ("<?php mb_substr([], 0);", "mb_substr() string argument must be string"),
        ("<?php mb_strcut();", "mb_strcut() takes 2 to 4 arguments"),
        ("<?php mb_strcut([], 0);", "mb_strcut() string argument must be string"),
        ("<?php mb_scrub();", "mb_scrub() takes 1 or 2 arguments"),
        ("<?php mb_scrub([]);", "mb_scrub() string argument must be string"),
        ("<?php mb_trim();", "mb_trim() takes 1 to 3 arguments"),
        ("<?php mb_trim([]);", "mb_trim() string argument must be string"),
        ("<?php mb_ltrim();", "mb_ltrim() takes 1 to 3 arguments"),
        ("<?php mb_ltrim([]);", "mb_ltrim() string argument must be string"),
        ("<?php mb_rtrim();", "mb_rtrim() takes 1 to 3 arguments"),
        ("<?php mb_rtrim([]);", "mb_rtrim() string argument must be string"),
        ("<?php mb_str_pad();", "mb_str_pad() takes 2 to 5 arguments"),
        ("<?php mb_str_pad([], 0);", "mb_str_pad() string argument must be string"),
        ("<?php mb_convert_kana();", "mb_convert_kana() takes 1 to 3 arguments"),
        ("<?php mb_convert_kana([]);", "mb_convert_kana() string argument must be string"),
        ("<?php mb_substr_count();", "mb_substr_count() takes 2 or 3 arguments"),
        ("<?php mb_substr_count([], \"x\");", "mb_substr_count() haystack argument must be string"),
        ("<?php mb_ord();", "mb_ord() takes 1 or 2 arguments"),
        ("<?php mb_ord([]);", "mb_ord() string argument must be string"),
        ("<?php mb_chr();", "mb_chr() takes 1 or 2 arguments"),
        ("<?php mb_chr([]);", "mb_chr() codepoint argument must be int"),
        ("<?php mb_strpos();", "mb_strpos() takes 2 to 4 arguments"),
        ("<?php mb_strpos([], \"x\");", "mb_strpos() haystack argument must be string"),
        ("<?php mb_stripos();", "mb_stripos() takes 2 to 4 arguments"),
        ("<?php mb_stripos([], \"x\");", "mb_stripos() haystack argument must be string"),
        ("<?php mb_strrpos();", "mb_strrpos() takes 2 to 4 arguments"),
        ("<?php mb_strrpos([], \"x\");", "mb_strrpos() haystack argument must be string"),
        ("<?php mb_strripos();", "mb_strripos() takes 2 to 4 arguments"),
        ("<?php mb_strripos([], \"x\");", "mb_strripos() haystack argument must be string"),
        ("<?php mb_strstr();", "mb_strstr() takes 2 to 4 arguments"),
        ("<?php mb_strstr([], \"x\");", "mb_strstr() haystack argument must be string"),
        ("<?php mb_stristr();", "mb_stristr() takes 2 to 4 arguments"),
        ("<?php mb_stristr([], \"x\");", "mb_stristr() haystack argument must be string"),
        ("<?php mb_strrchr();", "mb_strrchr() takes 2 to 4 arguments"),
        ("<?php mb_strrchr([], \"x\");", "mb_strrchr() haystack argument must be string"),
        ("<?php mb_strrichr();", "mb_strrichr() takes 2 to 4 arguments"),
        ("<?php mb_strrichr([], \"x\");", "mb_strrichr() haystack argument must be string"),
        ("<?php mb_language(null, null);", "mb_language() takes at most 1 argument"),
        ("<?php mb_language([]);", "mb_language() language argument must be string or null"),
        ("<?php mb_internal_encoding(null, null);", "mb_internal_encoding() takes at most 1 argument"),
        ("<?php mb_internal_encoding([]);", "mb_internal_encoding() encoding argument must be string or null"),
        ("<?php mb_http_output(null, null);", "mb_http_output() takes at most 1 argument"),
        ("<?php mb_http_output([]);", "mb_http_output() encoding argument must be string or null"),
        ("<?php declare(strict_types=1); mb_substr(\"a\", 0, \"bad\");", "mb_substr() length argument must be int or null"),
        ("<?php mb_strcut(\"a\", 0, []);", "mb_strcut() length argument must be int or null"),
        ("<?php mb_strstr(\"a\", \"a\", []);", "mb_strstr() before_needle argument must be bool"),
    ] {
        expect_error(source, message);
    }
}

/// Verifies false and boolean alternatives cannot disappear behind narrower declared returns.
#[test]
fn test_error_mbstring_union_returns_are_precise() {
    for (source, message) in [
        (r#"<?php function pos(): int { return mb_strpos("a", "z"); }"#,
            "Function 'pos' return type expects Int, got Union([Int, False])"),
        (r#"<?php function point(): int { return mb_ord("a"); }"#,
            "Function 'point' return type expects Int, got Union([Int, False])"),
        (r#"<?php function character(): string { return mb_chr(-1); }"#,
            "Function 'character' return type expects Str, got Union([Str, False])"),
        (r#"<?php function suffix(): string { return mb_strstr("a", "z"); }"#,
            "Function 'suffix' return type expects Str, got Union([Str, False])"),
        (r#"<?php function encoding(): string { return mb_internal_encoding(); }"#,
            "Function 'encoding' return type expects Str, got Union([Str, Bool])"),
    ] { expect_error(source, message); }
}

/// Verifies array and metadata builtins reject incorrect arity and static parameter types.
#[test]
fn test_error_mbstring_array_contracts() {
    for (source, message) in [
        ("<?php mb_substitute_character(null, null);", "mb_substitute_character() takes at most 1 argument"),
        ("<?php mb_substitute_character([]);", "mb_substitute_character() substitute_character argument must be string or int or null"),
        ("<?php mb_check_encoding(null, null, null);", "mb_check_encoding() takes at most 2 arguments"),
        ("<?php declare(strict_types=1); mb_check_encoding(true);", "mb_check_encoding() value argument must be array or string or null"),
        ("<?php mb_check_encoding([], []);", "mb_check_encoding() encoding argument must be string or null"),
        ("<?php mb_str_split();", "mb_str_split() takes 1 to 3 arguments"),
        ("<?php mb_str_split([]);", "mb_str_split() string argument must be string"),
        ("<?php mb_str_split(\"a\", []);", "mb_str_split() length argument must be int"),
        ("<?php mb_list_encodings(1);", "mb_list_encodings() takes no arguments"),
        ("<?php mb_list_encodings(...$missing);", "Undefined variable: $missing"),
        ("<?php mb_list_encodings(...42);", "Spread operator requires an array"),
        ("<?php mb_encoding_aliases();", "mb_encoding_aliases() takes exactly 1 argument"),
        ("<?php mb_encoding_aliases([]);", "mb_encoding_aliases() encoding argument must be string"),
        ("<?php mb_preferred_mime_name();", "mb_preferred_mime_name() takes exactly 1 argument"),
        ("<?php mb_preferred_mime_name([]);", "mb_preferred_mime_name() encoding argument must be string"),
    ] { expect_error(source, message); }
}

/// Rejects extra detection-order arguments and strict non-list inputs through the shared contract.
#[test]
fn test_error_mbstring_detect_order_contract() {
    expect_error("<?php mb_detect_order(null, null);", "mb_detect_order() takes at most 1 argument");
    expect_error("<?php declare(strict_types=1); mb_detect_order(123);",
        "mb_detect_order() encoding argument must be array or string or null");
}

/// Rejects wrong entity-map arity and statically impossible argument types.
#[test]
fn test_error_mbstring_entities_contracts() {
    for (source, message) in [
        ("<?php mb_encode_numericentity();", "mb_encode_numericentity() takes 2 to 4 arguments"),
        ("<?php mb_decode_numericentity(\"A\");", "mb_decode_numericentity() takes 2 or 3 arguments"),
        ("<?php mb_encode_numericentity(\"A\", \"bad\");", "mb_encode_numericentity() map argument must be array"),
        ("<?php mb_decode_numericentity([], []);", "mb_decode_numericentity() string argument must be string"),
        ("<?php mb_encode_numericentity(\"A\", [], [], false);", "mb_encode_numericentity() encoding argument must be string or null"),
        ("<?php mb_encode_numericentity(\"A\", [], null, []);", "mb_encode_numericentity() hex argument must be bool"),
    ] { expect_error(source, message); }
}

/// Enforces detection arity and static parameter shapes from the neutral PHP contract.
#[test]
fn test_error_mbstring_detect_encoding_contract() {
    for (source, message) in [
        ("<?php mb_detect_encoding();", "mb_detect_encoding() takes 1 to 3 arguments"),
        ("<?php mb_detect_encoding([], null);", "mb_detect_encoding() string argument must be string"),
        ("<?php declare(strict_types=1); mb_detect_encoding(\"A\", 123);", "mb_detect_encoding() encodings argument must be array or string or null"),
        ("<?php mb_detect_encoding(\"A\", null, []);", "mb_detect_encoding() strict argument must be bool"),
    ] { expect_error(source, message); }
}

/// Rejects wrong conversion arity and statically impossible encoding parameter shapes.
#[test]
fn test_error_mbstring_conversion_contract() {
    for (source, message) in [
        ("<?php mb_convert_encoding(\"A\");", "mb_convert_encoding() takes 2 or 3 arguments"),
        ("<?php mb_convert_encoding(\"A\", [], null);", "mb_convert_encoding() to_encoding argument must be string"),
        ("<?php declare(strict_types=1); mb_convert_encoding(123, \"UTF-8\");", "mb_convert_encoding() string argument must be array or string"),
        ("<?php declare(strict_types=1); mb_convert_encoding(\"A\", \"UTF-8\", 123);", "mb_convert_encoding() from_encoding argument must be array or string or null"),
    ] { expect_error(source, message); }
}
