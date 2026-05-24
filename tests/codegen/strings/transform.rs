//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings transform, including strtolower, strtoupper, and ucfirst.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_strtolower() {
    // Verifies strtolower converts all alphabetic characters to lowercase.
    let out = compile_and_run(r#"<?php echo strtolower("Hello WORLD");"#);
    assert_eq!(out, "hello world");
}

#[test]
fn test_strtoupper() {
    // Verifies strtoupper converts all alphabetic characters to uppercase.
    let out = compile_and_run(r#"<?php echo strtoupper("Hello World");"#);
    assert_eq!(out, "HELLO WORLD");
}

#[test]
fn test_ucfirst() {
    // Verifies ucfirst capitalizes the first character of a string.
    let out = compile_and_run(r#"<?php echo ucfirst("hello");"#);
    assert_eq!(out, "Hello");
}

#[test]
fn test_lcfirst() {
    // Verifies lcfirst lowercases the first character of a string.
    let out = compile_and_run(r#"<?php echo lcfirst("Hello");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_trim() {
    // Verifies trim removes whitespace from both ends of a string.
    let out = compile_and_run("<?php echo trim(\"  hello  \");");
    assert_eq!(out, "hello");
}

#[test]
fn test_ltrim() {
    // Verifies ltrim removes whitespace from the left end of a string.
    let out = compile_and_run("<?php echo ltrim(\"  hello\");");
    assert_eq!(out, "hello");
}

#[test]
fn test_rtrim() {
    // Verifies rtrim removes whitespace from the right end of a string.
    let out = compile_and_run("<?php echo rtrim(\"hello  \");");
    assert_eq!(out, "hello");
}

#[test]
fn test_str_repeat() {
    // Verifies str_repeat repeats a string the given number of times.
    let out = compile_and_run(r#"<?php echo str_repeat("ab", 3);"#);
    assert_eq!(out, "ababab");
}

#[test]
fn test_str_repeat_large_heap_backed_result() {
    // Verifies str_repeat handles large results that exceed the small-string inline buffer threshold (32768+ bytes), confirming the result is heap-allocated and its reported length is correct.
    let out = compile_and_run(
        r#"<?php
echo strlen(str_repeat("ab", 32769));
echo ",";
$s = str_repeat("ab", 33000);
echo strlen($s);
"#,
    );
    assert_eq!(out, "65538,66000");
}

#[test]
fn test_str_repeat_negative_count_reports_runtime_error() {
    // Verifies str_repeat emits a runtime error when given a negative count, matching PHP's behavior.
    let err = compile_and_run_expect_failure(r#"<?php echo str_repeat("ab", -1);"#);
    assert!(err.contains(
        "Fatal error: str_repeat(): Argument #2 ($times) must be greater than or equal to 0"
    ));
}

#[test]
fn test_strrev() {
    // Verifies strrev reverses the characters in a string.
    let out = compile_and_run(r#"<?php echo strrev("Hello");"#);
    assert_eq!(out, "olleH");
}

#[test]
fn test_str_replace() {
    // Verifies str_replace performs a simple find-and-replace on a string.
    let out = compile_and_run(r#"<?php echo str_replace("World", "PHP", "Hello World");"#);
    assert_eq!(out, "Hello PHP");
}

#[test]
fn test_str_replace_multiple() {
    // Verifies str_replace replaces all occurrences of a needle in a string.
    let out = compile_and_run(r#"<?php echo str_replace("o", "0", "Hello World");"#);
    assert_eq!(out, "Hell0 W0rld");
}

#[test]
fn test_explode() {
    // Verifies explode splits a string on a delimiter and returns an indexed array.
    let out = compile_and_run(
        r#"<?php
$parts = explode(",", "a,b,c");
echo count($parts);
echo " ";
echo $parts[0] . " " . $parts[1] . " " . $parts[2];
"#,
    );
    assert_eq!(out, "3 a b c");
}

#[test]
fn test_implode() {
    // Verifies implode joins array elements into a string with a given separator.
    let out = compile_and_run(
        r#"<?php
$arr = ["Hello", "World"];
echo implode(" ", $arr);
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_explode_implode_roundtrip() {
    // Verifies explode followed by implode produces the expected string transformation.
    let out = compile_and_run(
        r#"<?php
$str = "one-two-three";
$parts = explode("-", $str);
echo implode(", ", $parts);
"#,
    );
    assert_eq!(out, "one, two, three");
}

// --- v0.4 batch 2: more string functions ---

#[test]
fn test_ucwords() {
    // Verifies ucwords capitalizes the first character of each word in a string.
    let out = compile_and_run(r#"<?php echo ucwords("hello world foo");"#);
    assert_eq!(out, "Hello World Foo");
}

#[test]
fn test_str_ireplace() {
    // Verifies str_ireplace performs case-insensitive find-and-replace.
    let out = compile_and_run(r#"<?php echo str_ireplace("WORLD", "PHP", "Hello World");"#);
    assert_eq!(out, "Hello PHP");
}

#[test]
fn test_str_pad_right() {
    // Verifies str_pad with default right-padding when pad_type is omitted.
    let out = compile_and_run(r#"<?php echo str_pad("hi", 5);"#);
    assert_eq!(out, "hi   ");
}

#[test]
fn test_str_pad_left() {
    // Verifies str_pad left-padding when pad_type is explicitly 0 (left).
    let out = compile_and_run(r#"<?php echo str_pad("hi", 5, " ", 0);"#);
    assert_eq!(out, "   hi");
}

#[test]
fn test_str_pad_both() {
    // Verifies str_pad with pad_type 2 (both sides) and a custom pad character.
    let out = compile_and_run(r#"<?php echo str_pad("hi", 6, "-", 2);"#);
    assert_eq!(out, "--hi--");
}

#[test]
fn test_str_pad_custom_char() {
    // Verifies str_pad left-padding with a custom zero character.
    let out = compile_and_run(r#"<?php echo str_pad("42", 5, "0", 0);"#);
    assert_eq!(out, "00042");
}

#[test]
fn test_str_split() {
    // Verifies str_split splits a string into chunks of a given length.
    let out = compile_and_run(
        r#"<?php
$parts = str_split("Hello", 2);
echo count($parts) . " " . $parts[0] . " " . $parts[1] . " " . $parts[2];
"#,
    );
    assert_eq!(out, "3 He ll o");
}

#[test]
fn test_sprintf_zero_padded_int() {
    // Verifies sprintf zero-pads an integer to a given width.
    let out = compile_and_run(r#"<?php echo sprintf("%05d", 42);"#);
    assert_eq!(out, "00042");
}
