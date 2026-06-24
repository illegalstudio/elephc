//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings transform, including strtolower, strtoupper, and ucfirst.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies strtolower converts all alphabetic characters to lowercase.
#[test]
fn test_strtolower() {
    let out = compile_and_run(r#"<?php echo strtolower("Hello WORLD");"#);
    assert_eq!(out, "hello world");
}

/// Verifies strtoupper converts all alphabetic characters to uppercase.
#[test]
fn test_strtoupper() {
    let out = compile_and_run(r#"<?php echo strtoupper("Hello World");"#);
    assert_eq!(out, "HELLO WORLD");
}

/// Verifies ucfirst capitalizes the first character of a string.
#[test]
fn test_ucfirst() {
    let out = compile_and_run(r#"<?php echo ucfirst("hello");"#);
    assert_eq!(out, "Hello");
}

/// Verifies lcfirst lowercases the first character of a string.
#[test]
fn test_lcfirst() {
    let out = compile_and_run(r#"<?php echo lcfirst("Hello");"#);
    assert_eq!(out, "hello");
}

/// Verifies trim removes whitespace from both ends of a string.
#[test]
fn test_trim() {
    let out = compile_and_run("<?php echo trim(\"  hello  \");");
    assert_eq!(out, "hello");
}

/// Verifies ltrim removes whitespace from the left end of a string.
#[test]
fn test_ltrim() {
    let out = compile_and_run("<?php echo ltrim(\"  hello\");");
    assert_eq!(out, "hello");
}

/// Verifies rtrim removes whitespace from the right end of a string.
#[test]
fn test_rtrim() {
    let out = compile_and_run("<?php echo rtrim(\"hello  \");");
    assert_eq!(out, "hello");
}

/// Verifies str_repeat repeats a string the given number of times.
#[test]
fn test_str_repeat() {
    let out = compile_and_run(r#"<?php echo str_repeat("ab", 3);"#);
    assert_eq!(out, "ababab");
}

/// Verifies str_repeat handles large results that exceed the small-string inline buffer threshold (32768+ bytes), confirming the result is heap-allocated and its reported length is correct.
#[test]
fn test_str_repeat_large_heap_backed_result() {
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

/// Verifies str_repeat emits a runtime error when given a negative count, matching PHP's behavior.
#[test]
fn test_str_repeat_negative_count_reports_runtime_error() {
    let err = compile_and_run_expect_failure(r#"<?php echo str_repeat("ab", -1);"#);
    assert!(err.contains(
        "Fatal error: str_repeat(): Argument #2 ($times) must be greater than or equal to 0"
    ));
}

/// Verifies strrev reverses the characters in a string.
#[test]
fn test_strrev() {
    let out = compile_and_run(r#"<?php echo strrev("Hello");"#);
    assert_eq!(out, "olleH");
}

/// Verifies grapheme_strrev reverses ASCII text like strrev while returning the PHP string|false shape.
#[test]
fn test_grapheme_strrev_ascii() {
    let out = compile_and_run(r#"<?php echo grapheme_strrev("ABCDE");"#);
    assert_eq!(out, "EDCBA");
}

/// Verifies grapheme_strrev keeps a combining mark attached to its base character.
#[test]
fn test_grapheme_strrev_combining_mark_cluster() {
    let out = compile_and_run("<?php echo grapheme_strrev(\"Ae\\u{0301}B\");");
    assert_eq!(out, "Be\u{0301}A");
}

/// Verifies grapheme_strrev keeps emoji modifiers and ZWJ sequences together as one cluster.
#[test]
fn test_grapheme_strrev_emoji_modifier_zwj_cluster() {
    let out = compile_and_run("<?php echo grapheme_strrev(\"A\\u{1F469}\\u{1F3FD}\\u{200D}\\u{1F4BB}B\");");
    assert_eq!(out, "B\u{1F469}\u{1F3FD}\u{200D}\u{1F4BB}A");
}

/// Verifies grapheme_strrev preserves embedded NUL bytes while reversing surrounding clusters.
#[test]
fn test_grapheme_strrev_preserves_nul_bytes() {
    let out = compile_and_run(r#"<?php echo grapheme_strrev("ab\0cd");"#);
    assert_eq!(out.as_bytes(), b"dc\0ba");
}

/// Verifies grapheme_strrev participates in builtin lookup, namespace fallback, and first-class callable syntax.
#[test]
fn test_grapheme_strrev_lookup_and_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo function_exists("GrApHeMe_StRrEv") ? "1" : "0";
echo ":";
echo GrApHeMe_StRrEv("desk");
echo ":";
$rev = grapheme_strrev(...);
echo $rev("tool");
"#,
    );
    assert_eq!(out, "1:ksed:loot");
}

/// Verifies str_replace performs a simple find-and-replace on a string.
#[test]
fn test_str_replace() {
    let out = compile_and_run(r#"<?php echo str_replace("World", "PHP", "Hello World");"#);
    assert_eq!(out, "Hello PHP");
}

/// Verifies str_replace replaces all occurrences of a needle in a string.
#[test]
fn test_str_replace_multiple() {
    let out = compile_and_run(r#"<?php echo str_replace("o", "0", "Hello World");"#);
    assert_eq!(out, "Hell0 W0rld");
}

/// Verifies explode splits a string on a delimiter and returns an indexed array.
#[test]
fn test_explode() {
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

/// Verifies implode joins array elements into a string with a given separator.
#[test]
fn test_implode() {
    let out = compile_and_run(
        r#"<?php
$arr = ["Hello", "World"];
echo implode(" ", $arr);
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Verifies explode followed by implode produces the expected string transformation.
#[test]
fn test_explode_implode_roundtrip() {
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

/// Verifies ucwords capitalizes the first character of each word in a string.
#[test]
fn test_ucwords() {
    let out = compile_and_run(r#"<?php echo ucwords("hello world foo");"#);
    assert_eq!(out, "Hello World Foo");
}

/// Verifies str_ireplace performs case-insensitive find-and-replace.
#[test]
fn test_str_ireplace() {
    let out = compile_and_run(r#"<?php echo str_ireplace("WORLD", "PHP", "Hello World");"#);
    assert_eq!(out, "Hello PHP");
}

/// Verifies str_pad with default right-padding when pad_type is omitted.
#[test]
fn test_str_pad_right() {
    let out = compile_and_run(r#"<?php echo str_pad("hi", 5);"#);
    assert_eq!(out, "hi   ");
}

/// Verifies str_pad left-padding when pad_type is explicitly 0 (left).
#[test]
fn test_str_pad_left() {
    let out = compile_and_run(r#"<?php echo str_pad("hi", 5, " ", 0);"#);
    assert_eq!(out, "   hi");
}

/// Verifies str_pad with pad_type 2 (both sides) and a custom pad character.
#[test]
fn test_str_pad_both() {
    let out = compile_and_run(r#"<?php echo str_pad("hi", 6, "-", 2);"#);
    assert_eq!(out, "--hi--");
}

/// Verifies str_pad left-padding with a custom zero character.
#[test]
fn test_str_pad_custom_char() {
    let out = compile_and_run(r#"<?php echo str_pad("42", 5, "0", 0);"#);
    assert_eq!(out, "00042");
}

/// Verifies str_split splits a string into chunks of a given length.
#[test]
fn test_str_split() {
    let out = compile_and_run(
        r#"<?php
$parts = str_split("Hello", 2);
echo count($parts) . " " . $parts[0] . " " . $parts[1] . " " . $parts[2];
"#,
    );
    assert_eq!(out, "3 He ll o");
}

/// Verifies sprintf zero-pads an integer to a given width.
#[test]
fn test_sprintf_zero_padded_int() {
    let out = compile_and_run(r#"<?php echo sprintf("%05d", 42);"#);
    assert_eq!(out, "00042");
}

/// Regression: a string builtin applied to a boxed `Mixed` value inside a concatenation must
/// unbox the argument into the string ABI registers. Before the fix `strtoupper` read the stale
/// left-hand concat operand (`"L:"`) instead of the Mixed argument, producing `"L:L:"`.
#[test]
fn test_strtoupper_of_mixed_in_concatenation() {
    let out = compile_and_run(r#"<?php $j = json_decode('"widget"'); echo "L:" . strtoupper($j);"#);
    assert_eq!(out, "L:WIDGET");
}

/// Regression: the same unboxing applies across string-transform builtins taking a `Mixed`
/// argument (here `strtolower`, `strrev`, `ucfirst`), not just `strtoupper`.
#[test]
fn test_string_transforms_of_mixed_argument() {
    let out = compile_and_run(
        r#"<?php
        $h = json_decode('"HELLO"');
        $a = json_decode('"abc"');
        echo strtolower($h), "|", strrev($a), "|", ucfirst($a);
        "#,
    );
    assert_eq!(out, "hello|cba|Abc");
}

/// Regression: multi-argument string builtins must also unbox a `Mixed` string argument, whether
/// it is the subject (`str_replace` arg 3), the haystack (`strpos`), or the source (`explode`) —
/// not only the first argument. Before the fix these read stale string registers for a Mixed arg.
#[test]
fn test_multiarg_string_builtins_of_mixed_argument() {
    let out = compile_and_run(
        r#"<?php
        $m = json_decode('"hello world"');
        echo str_replace("o", "0", $m), "|", strpos($m, "world"), "|", implode(",", explode(" ", $m));
        "#,
    );
    assert_eq!(out, "hell0 w0rld|6|hello,world");
}

/// Regression: `$s = substr($s, 0, $n)` reassigning a string to a substring of ITSELF inside a
/// loop, where `$s` is re-read afterward (here by `strrpos`), used to corrupt `$s` — its first
/// bytes became garbage because `substr` returned a slice into the source buffer that the store
/// then freed. Persisting the slice to an owned copy fixes it. This is the Composer ClassLoader
/// namespace-walking idiom. Output cross-checked with `php -r`.
#[test]
fn test_substr_self_reassign_in_loop_does_not_corrupt() {
    let out = compile_and_run(
        r#"<?php
$s = "aaaa.bbbb.cccc.dddd.eeee";
$p = strrpos($s, ".");
while (false !== $p) {
    echo $p . ":" . $s . "\n";
    $s = substr($s, 0, $p);
    $p = strrpos($s, ".");
}
echo "root:" . $s . "\n";
"#,
    );
    assert_eq!(
        out,
        "19:aaaa.bbbb.cccc.dddd.eeee\n14:aaaa.bbbb.cccc.dddd\n9:aaaa.bbbb.cccc\n4:aaaa.bbbb\nroot:aaaa\n"
    );
}

/// Regression: a plain `$s = substr($s, ...)` self-reassign without any later read of `$s` must
/// keep producing the right value (guards the persisted-copy path for the simple case).
#[test]
fn test_substr_self_reassign_repeated() {
    let out = compile_and_run(
        r#"<?php
$s = "ABCDEFGHIJ";
$s = substr($s, 0, 8);
$s = substr($s, 0, 6);
$s = substr($s, 2);
echo $s;
"#,
    );
    assert_eq!(out, "CDEF");
}
