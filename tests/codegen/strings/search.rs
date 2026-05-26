//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings search, including substr basic, substr with length, and substr negative offset.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_substr_basic() {
    // Verifies substr extracts the suffix starting at a positive offset.
    // Fixture: "Hello World" with offset 6 returns "World".
    let out = compile_and_run(r#"<?php echo substr("Hello World", 6);"#);
    assert_eq!(out, "World");
}

#[test]
fn test_substr_with_length() {
    // Verifies substr respects a length parameter to limit the extraction.
    // Fixture: "Hello World" with offset 0 and length 5 returns "Hello".
    let out = compile_and_run(r#"<?php echo substr("Hello World", 0, 5);"#);
    assert_eq!(out, "Hello");
}

#[test]
fn test_substr_negative_offset() {
    // Verifies substr interprets a negative offset as distance from the end of the string.
    // Fixture: "Hello World" with offset -5 returns "World".
    let out = compile_and_run(r#"<?php echo substr("Hello World", -5);"#);
    assert_eq!(out, "World");
}

#[test]
fn test_substr_coerces_mixed_numeric_offset_from_function_return_add() {
    // Verifies substr accepts a non-negative integer offset derived from a function return via addition.
    // Regression test: int-to-integer coercion path for the offset expression `$o + 1`.
    // Fixture: queries with `?` delimiter, strpos + intval, then substr with +1 offset.
    let out = compile_and_run(
        r#"<?php
function get_index(string $s): int {
    $p = strpos($s, "?");
    return intval($p);
}
function slice_after(string $s): string {
    $o = get_index($s);
    $p = $o + 1;
    return substr($s, $p);
}
echo slice_after("/hello?name=elephc"), "\n";
echo substr("/hello?name=elephc", get_index("/hello?name=elephc") + 1), "\n";
"#,
    );
    assert_eq!(out, "name=elephc\nname=elephc\n");
}

#[test]
fn test_strpos_found() {
    // Verifies strpos returns the integer byte offset when the needle is found.
    // Fixture: "Hello World" contains "World" starting at offset 6.
    let out = compile_and_run(r#"<?php echo strpos("Hello World", "World");"#);
    assert_eq!(out, "6");
}

#[test]
fn test_strpos_not_found() {
    // Verifies strpos returns empty string when the needle is absent.
    // Fixture: "Hello" does not contain "xyz".
    let out = compile_and_run(r#"<?php echo strpos("Hello", "xyz");"#);
    assert_eq!(out, "");
}

#[test]
fn test_strpos_not_found_is_strict_false() {
    // Verifies strpos uses strict `=== false` comparison when the needle is not found.
    // Fixture: strpos on "Hello"/"xyz" is strict-false, not just falsy.
    let out = compile_and_run(r#"<?php echo strpos("Hello", "xyz") === false ? "miss" : "hit";"#);
    assert_eq!(out, "miss");
}

#[test]
fn test_strpos_assigned_not_found_is_strict_false() {
    // Verifies assignment of strpos result to a variable preserves strict-false semantics.
    // Fixture: `$pos = strpos(...)` then strict comparison against false.
    let out = compile_and_run(
        r#"<?php
$pos = strpos("Hello", "xyz");
echo $pos === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

#[test]
fn test_strpos_zero_offset_is_not_false() {
    // Verifies strpos returns 0 (not false) when the needle is at the start of the string.
    // Regression: zero is a valid offset and must not be confused with the false sentinel.
    // Fixture: "abc" contains "a" at offset 0, which is !== false.
    let out = compile_and_run(r#"<?php echo strpos("abc", "a") === false ? "miss" : "zero";"#);
    assert_eq!(out, "zero");
}

#[test]
fn test_strrpos() {
    // Verifies strrpos finds the last occurrence of a needle.
    // Fixture: "abcabc" last "bc" starts at offset 4.
    let out = compile_and_run(r#"<?php echo strrpos("abcabc", "bc");"#);
    assert_eq!(out, "4");
}

#[test]
fn test_strrpos_not_found_is_strict_false() {
    // Verifies strrpos returns strict false when the needle is absent.
    // Fixture: "abcabc" does not contain "zz".
    let out = compile_and_run(r#"<?php echo strrpos("abcabc", "zz") === false ? "miss" : "hit";"#);
    assert_eq!(out, "miss");
}

#[test]
fn test_strstr_found() {
    // Verifies strstr returns the portion of the string starting from the first needle occurrence.
    // Fixture: "user@example.com" split on "@" yields "@example.com".
    let out = compile_and_run(r#"<?php echo strstr("user@example.com", "@");"#);
    assert_eq!(out, "@example.com");
}

#[test]
fn test_strcmp_equal() {
    // Verifies strcmp returns 0 when two identical strings compare equal.
    let out = compile_and_run(r#"<?php echo strcmp("abc", "abc");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_strcmp_less() {
    // Verifies strcmp returns a negative value when the first string sorts before the second.
    // Fixture: "abc" < "abd" lexicographically.
    let out = compile_and_run(r#"<?php echo (strcmp("abc", "abd") < 0 ? "yes" : "no");"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_strcasecmp() {
    // Verifies strcasecmp performs case-insensitive string comparison, returning 0 for equal strings.
    let out = compile_and_run(r#"<?php echo strcasecmp("Hello", "hello");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_str_contains_true() {
    // Verifies str_contains returns 1 when the needle is present in the haystack.
    // Fixture: "Hello World" contains "World".
    let out = compile_and_run(r#"<?php echo str_contains("Hello World", "World");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_str_contains_false() {
    // Verifies str_contains returns empty string when the needle is absent.
    // Fixture: "Hello" does not contain "xyz".
    let out = compile_and_run(r#"<?php echo str_contains("Hello", "xyz");"#);
    assert_eq!(out, "");
}

#[test]
fn test_str_starts_with_true() {
    // Verifies str_starts_with returns 1 when the haystack starts with the needle.
    // Fixture: "Hello World" starts with "Hello".
    let out = compile_and_run(r#"<?php echo str_starts_with("Hello World", "Hello");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_str_starts_with_false() {
    // Verifies str_starts_with returns empty string when the haystack does not start with the needle.
    // Fixture: "Hello" does not start with "World".
    let out = compile_and_run(r#"<?php echo str_starts_with("Hello", "World");"#);
    assert_eq!(out, "");
}

#[test]
fn test_str_ends_with_true() {
    // Verifies str_ends_with returns 1 when the haystack ends with the needle.
    // Fixture: "Hello World" ends with "World".
    let out = compile_and_run(r#"<?php echo str_ends_with("Hello World", "World");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_str_ends_with_false() {
    // Verifies str_ends_with returns empty string when the haystack does not end with the needle.
    // Fixture: "Hello" does not end with "xyz".
    let out = compile_and_run(r#"<?php echo str_ends_with("Hello", "xyz");"#);
    assert_eq!(out, "");
}

#[test]
fn test_substr_replace() {
    // Verifies substr_replace replaces a substring at a given offset and length with the replacement string.
    // Fixture: "hello world" replaced at offset 6, length 5 with "PHP" yields "hello PHP".
    let out = compile_and_run(r#"<?php echo substr_replace("hello world", "PHP", 6, 5);"#);
    assert_eq!(out, "hello PHP");
}

#[test]
fn test_substr_replace_no_length() {
    // Verifies substr_replace replaces from offset to end of string when length is omitted.
    // Fixture: "hello world" replaced at offset 5 with "!" yields "hello!".
    let out = compile_and_run(r#"<?php echo substr_replace("hello world", "!", 5);"#);
    assert_eq!(out, "hello!");
}
