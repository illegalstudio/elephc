//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings search, including substr basic, substr with length, and substr negative offset.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies substr extracts the suffix starting at a positive offset.
/// Fixture: "Hello World" with offset 6 returns "World".
#[test]
fn test_substr_basic() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", 6);"#);
    assert_eq!(out, "World");
}

/// Verifies substr respects a length parameter to limit the extraction.
/// Fixture: "Hello World" with offset 0 and length 5 returns "Hello".
#[test]
fn test_substr_with_length() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", 0, 5);"#);
    assert_eq!(out, "Hello");
}

/// Verifies substr interprets a negative offset as distance from the end of the string.
/// Fixture: "Hello World" with offset -5 returns "World".
#[test]
fn test_substr_negative_offset() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", -5);"#);
    assert_eq!(out, "World");
}

/// A NEGATIVE length is php's "stop this many bytes before the end", and every row here used to
/// be wrong — silently, with a plausible-looking string.
///
/// Two faults compounded. `-1` doubled as the in-band sentinel for "no length argument", so an
/// explicit `substr($s, 1, -1)` was indistinguishable from the two-argument call and kept the
/// whole tail; and every other negative length was clamped to zero, so `substr("hello", 0, -2)`
/// answered `""` where php answers `"hel"`. Whether a length was PASSED is known from the
/// operand count at compile time and is never encoded in the length's own value now.
///
/// The controls matter as much as the fixes: the two-argument form, a zero length, an
/// over-long length and an out-of-range offset all have to keep their previous answers, since
/// removing the sentinel touched the path they share.
#[test]
fn test_substr_negative_length_omits_bytes_from_the_end() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    substr("hello", 1, -1),   // the row the -1 sentinel swallowed
    substr("hello", 0, -2),
    substr("hello", 1, -2),
    substr("hello", -4, -1),  // negative offset AND negative length
    substr("hello", 0, -5),   // omits exactly everything
    substr("hello", 0, -9),   // omits more than there is
    substr("hello", 1),       // control: two-argument form
    substr("hello", 1, 2),    // control: ordinary length
    substr("hello", 1, 0),    // control: empty selection
    substr("hello", 1, 99),   // control: length past the end
    substr("hello", -3, 2),   // control: negative offset, positive length
    substr("hello", 9, 2),    // control: offset past the end
];
echo implode("|", $rows);
"#,
    );
    assert_eq!(out, "ell|hel|el|ell|||ello|el||ello|ll|");
}

/// The same rule for `substr_replace()`, whose omitted-length signal had the same collision.
///
/// A negative length told it to replace nothing (`"hX"` for `substr_replace("hello","X",1,-1)`,
/// where php answers `"hXo"`). The omitted-length case now reaches the runtime helper as
/// `i64::MAX` instead of `-1`: the helper bounds the length by the remaining tail, so a
/// saturating value runs through the end by the ordinary path and needs no sentinel test —
/// which frees `-1` to mean what php means by it.
#[test]
fn test_substr_replace_negative_length_omits_bytes_from_the_end() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    substr_replace("hello", "X", 1, -1),
    substr_replace("hello", "X", 0, -2),
    substr_replace("hello", "X", 1, -3),
    substr_replace("hello", "X", -3, -1),  // negative offset AND negative length
    substr_replace("hello", "X", 1, -9),   // omits more than remains
    substr_replace("hello", "X", 1),       // control: omitted length
    substr_replace("hello", "X", 1, 0),    // control: pure insertion
    substr_replace("hello", "X", 1, 2),    // control: ordinary length
    substr_replace("hello", "X", 1, 99),   // control: length past the end
    substr_replace("hello", "X", 9, 2),    // control: offset past the end
];
echo implode("|", $rows);
"#,
    );
    assert_eq!(out, "hXo|Xlo|hXllo|heXo|hXello|hX|hXello|hXlo|hX|helloX");
}

/// Verifies explicit null and named omitted/null lengths run through the string end.
///
/// Zero-length controls remain empty selection for `substr()` and pure insertion for
/// `substr_replace()`, keeping a real zero distinct from PHP's nullable default.
#[test]
fn test_substr_and_substr_replace_null_length_runs_to_end() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    substr("hello", 1),
    substr("hello", 1, null),
    substr(string: "hello", offset: 1),
    substr(string: "hello", offset: 1, length: null),
    substr("hello", 1, 0),
    substr_replace("hello", "X", 1),
    substr_replace("hello", "X", 1, null),
    substr_replace(string: "hello", replace: "X", offset: 1),
    substr_replace(string: "hello", replace: "X", offset: 1, length: null),
    substr_replace("hello", "X", 1, 0),
];
echo implode("|", $rows);
"#,
    );
    assert_eq!(out, "ello|ello|ello|ello||hX|hX|hX|hX|hXello");
}

/// Verifies nullable lengths selected at runtime keep `null`, zero, and negative distinct.
///
/// The same `?int` fixture runs through tagged and sentinel null representations. A boxed
/// `mixed` null also covers the dynamic path used by `substr_replace()` and `substr_count()`.
/// Null selects omitted-length behavior while zero and negative values remain concrete lengths.
#[test]
fn test_substring_builtins_runtime_null_length_runs_to_end() {
    let source = r#"<?php
function runtime_length(int $selector): ?int {
    if ($selector === 1) {
        return null;
    }
    if ($selector === 2) {
        return 0;
    }
    return -1;
}

function runtime_mixed_length(int $selector): mixed {
    if ($selector === 1) {
        return null;
    }
    if ($selector === 2) {
        return 0;
    }
    return -1;
}

$null = runtime_length($argc);
$zero = runtime_length($argc + 1);
$negative = runtime_length($argc + 2);
$mixed_null = runtime_mixed_length($argc);
$mixed_zero = runtime_mixed_length($argc + 1);
$mixed_negative = runtime_mixed_length($argc + 2);

echo substr("hello", 1, $null), "|",
     substr_replace("hello", "X", 1, $null), "|",
     substr_count("hello world", "o", 0, $null), "\n";
echo substr("hello", 1, $zero), "|",
     substr_replace("hello", "X", 1, $zero), "|",
     substr_count("hello world", "o", 0, $zero), "\n";
echo substr("hello", 1, $negative), "|",
     substr_replace("hello", "X", 1, $negative), "|",
     substr_count("hello world", "o", 0, $negative), "\n";
echo substr("hello", 1, $mixed_null), "|",
     substr_replace("hello", "X", 1, $mixed_null), "|",
     substr_count("hello world", "o", 0, $mixed_null), "\n";
echo substr("hello", 1, $mixed_zero), "|",
     substr_replace("hello", "X", 1, $mixed_zero), "|",
     substr_count("hello world", "o", 0, $mixed_zero), "\n";
echo substr("hello", 1, $mixed_negative), "|",
     substr_replace("hello", "X", 1, $mixed_negative), "|",
     substr_count("hello world", "o", 0, $mixed_negative), "\n";
"#;
    let tagged = compile_and_run_tagged(source);
    let sentinel = compile_and_run_sentinel(source);
    let expected =
        "ello|hX|2\n|hXello|0\nell|hXo|2\nello|hX|2\n|hXello|0\nell|hXo|2\n";
    assert_eq!(tagged, expected);
    assert_eq!(sentinel, expected);
}

/// Verifies substr accepts a non-negative integer offset derived from a function return via addition.
/// Regression test: int-to-integer coercion path for the offset expression `$o + 1`.
/// Fixture: queries with `?` delimiter, strpos + intval, then substr with +1 offset.
#[test]
fn test_substr_coerces_mixed_numeric_offset_from_function_return_add() {
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

/// Uses shortest-round-trip formatting for a lossy weak float-to-int deprecation.
#[test]
fn test_substr_float_offset_deprecation_preserves_exact_value() {
    let out = compile_and_run_capture(
        r#"<?php
$offset = (0.1 + 0.2) * $argc;
echo substr("abc", $offset);
"#,
    );
    assert_eq!(out.stdout, "abc");
    assert!(
        out.stderr.contains(
            "Deprecated: Implicit conversion from float 0.30000000000000004 to int loses precision"
        ),
        "{}",
        out.stderr
    );
}

/// Rejects NaN, infinity, and out-of-range floats at a weak int argument boundary.
#[test]
fn test_substr_nonrepresentable_float_offset_throws_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$n = $argc;
foreach ([INF * $n, NAN * $n, 1e20 * $n] as $offset) {
    try { echo substr("abc", $offset); }
    catch (TypeError $error) { echo $error->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out.stdout,
        "substr(): Argument #2 ($offset) must be of type int, float given\n\
substr(): Argument #2 ($offset) must be of type int, float given\n\
substr(): Argument #2 ($offset) must be of type int, float given\n",
        "success={} stderr={}",
        out.success,
        out.stderr
    );
    assert_eq!(out.stderr, "");
}

/// Verifies strpos returns the integer byte offset when the needle is found.
/// Fixture: "Hello World" contains "World" starting at offset 6.
#[test]
fn test_strpos_found() {
    let out = compile_and_run(r#"<?php echo strpos("Hello World", "World");"#);
    assert_eq!(out, "6");
}

/// Verifies strpos returns empty string when the needle is absent.
/// Fixture: "Hello" does not contain "xyz".
#[test]
fn test_strpos_not_found() {
    let out = compile_and_run(r#"<?php echo strpos("Hello", "xyz");"#);
    assert_eq!(out, "");
}

/// Verifies strpos uses strict `=== false` comparison when the needle is not found.
/// Fixture: strpos on "Hello"/"xyz" is strict-false, not just falsy.
#[test]
fn test_strpos_not_found_is_strict_false() {
    let out = compile_and_run(r#"<?php echo strpos("Hello", "xyz") === false ? "miss" : "hit";"#);
    assert_eq!(out, "miss");
}

/// Verifies assignment of strpos result to a variable preserves strict-false semantics.
/// Fixture: `$pos = strpos(...)` then strict comparison against false.
#[test]
fn test_strpos_assigned_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$pos = strpos("Hello", "xyz");
echo $pos === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Verifies strpos returns 0 (not false) when the needle is at the start of the string.
/// Regression: zero is a valid offset and must not be confused with the false sentinel.
/// Fixture: "abc" contains "a" at offset 0, which is !== false.
#[test]
fn test_strpos_zero_offset_is_not_false() {
    let out = compile_and_run(r#"<?php echo strpos("abc", "a") === false ? "miss" : "zero";"#);
    assert_eq!(out, "zero");
}

/// Verifies strrpos finds the last occurrence of a needle.
/// Fixture: "abcabc" last "bc" starts at offset 4.
#[test]
fn test_strrpos() {
    let out = compile_and_run(r#"<?php echo strrpos("abcabc", "bc");"#);
    assert_eq!(out, "4");
}

/// Verifies strrpos returns strict false when the needle is absent.
/// Fixture: "abcabc" does not contain "zz".
#[test]
fn test_strrpos_not_found_is_strict_false() {
    let out = compile_and_run(r#"<?php echo strrpos("abcabc", "zz") === false ? "miss" : "hit";"#);
    assert_eq!(out, "miss");
}

/// Verifies strrchr returns the suffix at the final match, uses only the first needle byte, and
/// preserves PHP's false result when no match exists.
#[test]
fn test_strrchr_suffix_first_needle_byte_and_miss() {
    let out = compile_and_run(
        r#"<?php
echo strrchr('A\\B\\C', '\\'), '|';
echo strrchr('abc', 'bc'), '|';
echo strrchr('abc', 'z') === false ? 'F' : 'bad';
"#,
    );
    assert_eq!(out, "\\C|bc|F");
}

/// Verifies `strpbrk()` selects the first byte from its character set and preserves `false`.
#[test]
fn test_strpbrk_suffix_miss_binary_bytes_and_empty_characters_error() {
    let out = compile_and_run(
        r#"<?php
echo strpbrk("abc123", "32"), "|";
echo \STRPBRK(string: "abc", characters: "z") === false ? "F" : "bad", "|";
echo strpbrk("a".chr(0)."b", chr(0)), "|";
try {
    strpbrk("abc", "");
} catch (ValueError $error) {
    echo get_class($error), ":", $error->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "23|F|\0b|ValueError:strpbrk(): Argument #2 ($characters) must be a non-empty string"
    );
}

/// Verifies php-src byte-span semantics, bounds, naming, and namespace fallback.
#[test]
fn test_strcspn_and_strspn_php_src_semantics() {
    let out = compile_and_run(
        r#"<?php
namespace SpanFixture;

function spanWithNullableLength(?int $length): int {
    return strcspn("abc123", "123", 1, $length);
}

$dynamicCspn = "strcspn";
$dynamicSpn = \strspn(...);

$subject = "22222222aaaa bbb1111 cccc";
$characters = "1234";

echo STRCSPN($subject, $characters), "|",
     strcspn($subject, $characters, 9), "|",
     \strcspn($subject, $characters, 9, 6), "|",
     strcspn("abc123", "123", offset: 1, length: -1), "|",
     strcspn("abc\0def", "\0"), "|",
     strcspn("abc", ""), "|",
     strspn($subject, $characters), "|",
     strspn($subject, $characters, 2), "|",
     \STRSPN($subject, $characters, 2, 3), "|",
     strspn(string: "abc123", characters: "abc", offset: -20, length: null), "|",
     strspn("abc", ""), "|",
     spanWithNullableLength(null), "|",
     spanWithNullableLength(-1), "|",
     $dynamicCspn("abc123", "123"), "|",
     $dynamicSpn("abc123", "abc"), "|",
     call_user_func_array("strcspn", ["abc123", "123", 1, 4]), "|",
     strcspn("abc", "x", PHP_INT_MIN), "|",
     strcspn("abc", "x", PHP_INT_MAX), "|",
     strcspn("abc", "x", 0, PHP_INT_MIN), "|",
     strcspn("abc", "x", 0, PHP_INT_MAX);
"#,
    );
    assert_eq!(out, "0|7|6|2|3|3|8|6|3|3|0|2|2|3|3|2|3|0|0|3");
}

/// Verifies strstr returns the portion of the string starting from the first needle occurrence.
/// Fixture: "user@example.com" split on "@" yields "@example.com".
#[test]
fn test_strstr_found() {
    let out = compile_and_run(r#"<?php echo strstr("user@example.com", "@");"#);
    assert_eq!(out, "@example.com");
}

/// Verifies strcmp returns 0 when two identical strings compare equal.
#[test]
fn test_strcmp_equal() {
    let out = compile_and_run(r#"<?php echo strcmp("abc", "abc");"#);
    assert_eq!(out, "0");
}

/// Verifies strcmp returns a negative value when the first string sorts before the second.
/// Fixture: "abc" < "abd" lexicographically.
#[test]
fn test_strcmp_less() {
    let out = compile_and_run(r#"<?php echo (strcmp("abc", "abd") < 0 ? "yes" : "no");"#);
    assert_eq!(out, "yes");
}

/// Verifies strcasecmp performs case-insensitive string comparison, returning 0 for equal strings.
#[test]
fn test_strcasecmp() {
    let out = compile_and_run(r#"<?php echo strcasecmp("Hello", "hello");"#);
    assert_eq!(out, "0");
}

/// Verifies str_contains returns 1 when the needle is present in the haystack.
/// Fixture: "Hello World" contains "World".
#[test]
fn test_str_contains_true() {
    let out = compile_and_run(r#"<?php echo str_contains("Hello World", "World");"#);
    assert_eq!(out, "1");
}

/// Verifies str_contains returns empty string when the needle is absent.
/// Fixture: "Hello" does not contain "xyz".
#[test]
fn test_str_contains_false() {
    let out = compile_and_run(r#"<?php echo str_contains("Hello", "xyz");"#);
    assert_eq!(out, "");
}

/// Verifies str_starts_with returns 1 when the haystack starts with the needle.
/// Fixture: "Hello World" starts with "Hello".
#[test]
fn test_str_starts_with_true() {
    let out = compile_and_run(r#"<?php echo str_starts_with("Hello World", "Hello");"#);
    assert_eq!(out, "1");
}

/// Verifies str_starts_with returns empty string when the haystack does not start with the needle.
/// Fixture: "Hello" does not start with "World".
#[test]
fn test_str_starts_with_false() {
    let out = compile_and_run(r#"<?php echo str_starts_with("Hello", "World");"#);
    assert_eq!(out, "");
}

/// Verifies str_ends_with returns 1 when the haystack ends with the needle.
/// Fixture: "Hello World" ends with "World".
#[test]
fn test_str_ends_with_true() {
    let out = compile_and_run(r#"<?php echo str_ends_with("Hello World", "World");"#);
    assert_eq!(out, "1");
}

/// Verifies str_ends_with returns empty string when the haystack does not end with the needle.
/// Fixture: "Hello" does not end with "xyz".
#[test]
fn test_str_ends_with_false() {
    let out = compile_and_run(r#"<?php echo str_ends_with("Hello", "xyz");"#);
    assert_eq!(out, "");
}

/// Verifies substr_replace replaces a substring at a given offset and length with the replacement string.
/// Fixture: "hello world" replaced at offset 6, length 5 with "PHP" yields "hello PHP".
#[test]
fn test_substr_replace() {
    let out = compile_and_run(r#"<?php echo substr_replace("hello world", "PHP", 6, 5);"#);
    assert_eq!(out, "hello PHP");
}

/// Verifies substr_replace replaces from offset to end of string when length is omitted.
/// Fixture: "hello world" replaced at offset 5 with "!" yields "hello!".
#[test]
fn test_substr_replace_no_length() {
    let out = compile_and_run(r#"<?php echo substr_replace("hello world", "!", 5);"#);
    assert_eq!(out, "hello!");
}

/// Verifies `substr_replace()` stringifies a nullable integer subject through the TaggedScalar
/// representation before dispatching to the string runtime helper.
#[test]
fn test_substr_replace_tagged_scalar_subject() {
    let out = compile_and_run(
        r#"<?php
function replace_subject(?int $subject): string {
    return substr_replace($subject, "X", 0, 1);
}
echo replace_subject(123);
"#,
    );
    assert_eq!(out, "X23");
}

/// Verifies `substr_count()` counts non-overlapping occurrences.
/// `LC_ALL=C php` prints `2` for both `substr_count("hello world", "o")` and
/// `substr_count("aaaa", "aa")` — matches never overlap.
#[test]
fn test_substr_count_non_overlapping() {
    let out = compile_and_run(
        r#"<?php echo substr_count("hello world", "o"), "|", substr_count("aaaa", "aa"), "|", substr_count("hello", "z");"#,
    );
    assert_eq!(out, "2|2|0");
}

/// Verifies `substr_count()` honours the `$offset` argument, including a negative offset
/// measured back from the subject end. `LC_ALL=C php` prints `1` for both forms.
#[test]
fn test_substr_count_offset() {
    let out = compile_and_run(
        r#"<?php echo substr_count("hello world", "o", 5), "|", substr_count("hello world", "o", -5);"#,
    );
    assert_eq!(out, "1|1");
}

/// Verifies `substr_count()` honours `$length`, including the negative form measured back
/// from the subject end, and treats an explicit `null` like an omitted argument.
/// `LC_ALL=C php` prints `1`, `1`, `1`, `2`.
#[test]
fn test_substr_count_length() {
    let out = compile_and_run(
        r#"<?php
echo substr_count("hello world", "o", 0, 5), "|",
     substr_count("hello world", "o", 0, -5), "|",
     substr_count("hello world", "l", 3, 4), "|",
     substr_count("hello world", "o", 0, null);
"#,
    );
    assert_eq!(out, "1|1|1|2");
}

/// Verifies `substr_count()` resolves case-insensitively, through a namespace-qualified
/// call, and by named argument.
#[test]
fn test_substr_count_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php
echo SUBSTR_COUNT("hello world", "o"), "|",
     \substr_count("hello world", "o"), "|",
     substr_count(haystack: "hello world", needle: "o", offset: 5);
"#,
    );
    assert_eq!(out, "2|2|1");
}

/// Verifies `substr_count()` raises php-src's catchable `ValueError`s for an empty needle
/// and for an `$offset`/`$length` pair that leaves the subject. Messages are verbatim
/// `LC_ALL=C php` 8.4 output.
#[test]
fn test_substr_count_value_errors() {
    let out = compile_and_run(
        r#"<?php
foreach ([["abc", "", 0, null], ["abc", "b", 5, null], ["abc", "b", 0, 9]] as $t) {
    try {
        substr_count($t[0], $t[1], $t[2], $t[3]);
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "substr_count(): Argument #2 ($needle) must not be empty\n\
substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)\n"
    );
}

/// Verifies `strncmp()` compares only the first `$length` bytes and returns php-src's raw
/// byte difference. `LC_ALL=C php` prints `0`, `-12`, `-1`, `1`, `0` for these calls.
#[test]
fn test_strncmp_prefix_and_byte_difference() {
    let out = compile_and_run(
        r#"<?php
echo strncmp("Hello", "Hexxx", 2), "|",
     strncmp("Hello", "Hexxx", 3), "|",
     strncmp("abc", "abd", 3), "|",
     strncmp("abc", "ab", 3), "|",
     strncmp("abc", "abc", 10);
"#,
    );
    assert_eq!(out, "0|-12|-1|1|0");
}

/// Verifies `strncasecmp()` folds ASCII case before comparing the bounded prefix.
/// `LC_ALL=C php` prints `0`, `-1`, `1`.
#[test]
fn test_strncasecmp_ascii_folding() {
    let out = compile_and_run(
        r#"<?php
echo strncasecmp("HeLLo", "hellO", 5), "|",
     strncasecmp("ABC", "abd", 3), "|",
     strncasecmp("abc", "AB", 3);
"#,
    );
    assert_eq!(out, "0|-1|1");
}

/// Verifies both length-limited comparisons resolve case-insensitively, through a
/// namespace-qualified call, and by named argument.
#[test]
fn test_strncmp_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php
echo STRNCMP("abc", "abd", 3), "|",
     \strncasecmp("ABC", "abc", 3), "|",
     strncmp(string1: "abc", string2: "abd", length: 2);
"#,
    );
    assert_eq!(out, "-1|0|0");
}

/// Verifies both length-limited comparisons raise php-src's catchable `ValueError` for a
/// negative `$length`. Messages are verbatim `LC_ALL=C php` 8.4 output.
#[test]
fn test_strncmp_negative_length_value_errors() {
    let out = compile_and_run(
        r#"<?php
try { strncmp("a", "b", -1); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { strncasecmp("a", "b", -1); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "strncmp(): Argument #3 ($length) must be greater than or equal to 0\n\
strncasecmp(): Argument #3 ($length) must be greater than or equal to 0\n"
    );
}

/// Verifies `substr_compare()` returns php's own two-part answer: `memcmp()`'s RAW unsigned-byte
/// difference at the first mismatch, and `ZEND_THREEWAY_COMPARE`'s `-1`/`0`/`1` when one operand
/// is a prefix of the other. A normalized sign everywhere would break `< 0` callers; a length
/// DIFFERENCE in the tiebreak would break nothing here but does break `"ab"` vs `"abcdef"`, which
/// the prefix cases below pin.
/// `php` 8.5.10 prints `0`, `-1`, `-25`, `25`, `-32`, `-1`, `1`, `3` for these calls.
#[test]
fn test_substr_compare_byte_difference_and_threeway_tiebreak() {
    let out = compile_and_run(
        r#"<?php
echo substr_compare("abcde", "abcde", 0), "|",
     substr_compare("abcde", "abcdf", 0), "|",
     substr_compare("a", "z", 0), "|",
     substr_compare("z", "a", 0), "|",
     substr_compare("A", "a", 0), "|",
     substr_compare("abc", "abcdef", 0), "|",
     substr_compare("abcdef", "abc", 0), "|",
     substr_compare("abcdef", "abc", 3, 3);
"#,
    );
    assert_eq!(out, "0|-1|-25|25|-32|-1|1|3");
}

/// Verifies a negative `$offset` counts back from the haystack end and CLAMPS to zero when its
/// magnitude overruns the haystack, instead of raising the way `substr_count()` does for the same
/// input. `substr_compare("abcdef","def",-100)` is `-3` ('a' minus 'd'), which only holds if the
/// comparison really restarted at offset zero.
/// `php` 8.5.10 prints `0`, `0`, `0`, `0`, `-3`, `1`.
#[test]
fn test_substr_compare_negative_offset_clamps_instead_of_throwing() {
    let out = compile_and_run(
        r#"<?php
echo substr_compare("abcdef", "def", -3), "|",
     substr_compare("abcdef", "ef", -2), "|",
     substr_compare("abcdef", "abcdef", -6), "|",
     substr_compare("abcdef", "abcdef", -7), "|",
     substr_compare("abcdef", "def", -100), "|",
     substr_compare("abc", "a", PHP_INT_MIN);
"#,
    );
    assert_eq!(out, "0|0|0|0|-3|1");
}

/// Verifies a `null` `$length` compares the LONGER of the two operands (php's
/// `MAX(strlen($needle), strlen($haystack) - $offset)`), so a prefix answers `-1`/`1` where an
/// explicit `$length` covering only the prefix answers `0`. The final call proves a run-time
/// `null` reaching the argument is read as "no length" and not coerced to `0`, which would make
/// every call compare zero bytes and answer `0`.
/// `php` 8.5.10 prints `-1`, `0`, `-1`, `0`, `1`, `0`, `1`.
#[test]
fn test_substr_compare_length_null_explicit_and_runtime_null() {
    let out = compile_and_run(
        r#"<?php
function f(?int $l) { return substr_compare("abcdef", "abc", 0, $l); }
$n = null;
echo substr_compare("abcde", "abcdef", 0, null), "|",
     substr_compare("abcde", "abcdef", 0, 5), "|",
     substr_compare("abcde", "abcdef", 0, 100), "|",
     substr_compare("abcdef", "abc", 0, 3), "|",
     substr_compare("abcdef", "abc", 0), "|",
     substr_compare("abcdef", "abc", 0, 0), "|",
     f($n);
"#,
    );
    assert_eq!(out, "-1|0|-1|0|1|0|1");
}

/// Verifies `$case_insensitive` folds ASCII letters only. `[`/`{` and `_`/`?` differ by the same
/// `0x20` bit an ASCII letter pair does, so a fold that tested the bit rather than the `A`-`Z`
/// range would wrongly answer `0` for them. The UTF-8 pair shows multi-byte input is compared
/// byte for byte: the lead byte cancels and the continuation byte decides.
/// `php` 8.5.10 prints `0`, `0`, `-32`, `-32`, `32`, `-32`.
#[test]
fn test_substr_compare_case_insensitive_folds_ascii_only() {
    let out = compile_and_run(
        r#"<?php
echo substr_compare("Hello", "hello", 0, 5, true), "|",
     substr_compare("Hello", "hello", 0, null, true), "|",
     substr_compare("HELLO", "hello", 0, 5, false), "|",
     substr_compare("[", "{", 0, 1, true), "|",
     substr_compare("_", "?", 0, 1, true), "|",
     substr_compare("\xC3\x89", "\xC3\xA9", 0, 2, true);
"#,
    );
    assert_eq!(out, "0|0|-32|-32|32|-32");
}

/// Verifies bytes above `0x7F` are compared as UNSIGNED values the way `memcmp` does, and that
/// every empty-operand pairing is decided by the truncated-length tiebreak. Reading the bytes as
/// signed `char` would sort `"\xFF"` below `"\x01"` and flip the sign of every non-ASCII compare.
/// `php` 8.5.10 prints `94`, `-94`, `254`, `-254`, `-1`, `1`, `1`, `0`.
#[test]
fn test_substr_compare_unsigned_high_bytes_and_empty_operands() {
    let out = compile_and_run(
        r#"<?php
echo substr_compare("\xC3\xA9", "e", 0, 1), "|",
     substr_compare("e", "\xC3\xA9", 0, 1), "|",
     substr_compare("\xFF", "\x01", 0, 1), "|",
     substr_compare("\x01", "\xFF", 0, 1), "|",
     substr_compare("", "abc", 0), "|",
     substr_compare("abc", "", 0), "|",
     substr_compare("abc", "", 1), "|",
     substr_compare("", "", 0);
"#,
    );
    assert_eq!(out, "94|-94|254|-254|-1|1|1|0");
}

/// Verifies `substr_compare()` raises php-src's catchable `ValueError`s, in php's own order:
/// `$length` is refused first (a negative one is never measured back from the end, unlike
/// `substr_count()`'s), and only a call whose `$length` is valid reaches the `$offset` bound.
/// `$offset === strlen($haystack)` stays legal.
/// Messages are verbatim `php` 8.5.10 output.
#[test]
fn test_substr_compare_value_errors() {
    let out = compile_and_run(
        r#"<?php
foreach ([["abcdef", "abc", 0, -1], ["abc", "a", 100, -1], ["abc", "a", 100, 1], ["abc", "a", 3, 1]] as $t) {
    try {
        echo substr_compare($t[0], $t[1], $t[2], $t[3]), "\n";
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "substr_compare(): Argument #4 ($length) must be greater than or equal to 0\n\
substr_compare(): Argument #4 ($length) must be greater than or equal to 0\n\
substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
-1\n"
    );
}

/// Verifies `substr_compare()` resolves case-insensitively, through a root-namespace qualified
/// call, by named argument, and with a named argument that skips `$length` and leaves it at its
/// `null` default.
/// `php` 8.5.10 prints `0` for all four.
#[test]
fn test_substr_compare_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php
echo SUBSTR_COMPARE("abcdef", "def", -3), "|",
     \substr_compare("abcdef", "def", -3), "|",
     substr_compare(haystack: "Hello", needle: "hello", offset: 0, length: 5, case_insensitive: true), "|",
     substr_compare("Hello", "hello", 0, case_insensitive: true);
"#,
    );
    assert_eq!(out, "0|0|0|0");
}

/// Verifies `join()`, `substr_count()`, `strncmp()`, and `strncasecmp()` keep their PHP
/// types inside an array literal, whose element typing uses the checker's syntactic
/// inference table rather than the per-call checked type.
#[test]
fn test_new_string_builtins_keep_their_types_inside_array_literals() {
    let out = compile_and_run(
        r#"<?php
var_dump([join("-", ["a", "b"]), substr_count("aaa", "a"), strncmp("a", "b", 1), strncasecmp("A", "a", 1)]);
"#,
    );
    assert_eq!(
        out,
        "array(4) {\n  [0]=>\n  string(3) \"a-b\"\n  [1]=>\n  int(3)\n  [2]=>\n  int(-1)\n  [3]=>\n  int(0)\n}\n"
    );
}

/// Verifies `strpos()` accepts PHP's third `$offset` argument positionally and by name, and
/// resolves a negative offset against the haystack length.
/// Expected values are verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_strpos_offset_positional_and_named() {
    let out = compile_and_run(
        r#"<?php
var_dump(strpos("hello world", "o"));
var_dump(strpos("hello world", "o", 5));
var_dump(strpos("hello world", "o", -4));
var_dump(strpos("hello world", "o", offset: 5));
var_dump(strpos("hello world", "o", offset: -4));
var_dump(strpos("abc", "", 1));
var_dump(strpos("abc", "", 3));
var_dump(strpos("abc", "a", 3));
var_dump(strpos("hello", "z", 2));
"#,
    );
    assert_eq!(
        out,
        "int(4)\nint(7)\nint(7)\nint(7)\nint(7)\nint(1)\nint(3)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies `strrpos()` accepts PHP's third `$offset` argument positionally and by name.
/// A non-negative offset starts the right-to-left scan there, while a negative one bounds
/// where a match may end, so `strrpos("abcabc", "bc", -3)` finds the earlier match.
/// Expected values are verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_strrpos_offset_positional_and_named() {
    let out = compile_and_run(
        r#"<?php
var_dump(strrpos("hello world", "o", 5));
var_dump(strrpos("hello world", "o", 8));
var_dump(strrpos("hello world", "o", -3));
var_dump(strrpos("hello world", "o", offset: -3));
var_dump(strrpos("abcabc", "bc", -2));
var_dump(strrpos("abcabc", "bc", -3));
var_dump(strrpos("abcabc", "bc", -6));
var_dump(strrpos("abc", "", 1));
var_dump(strrpos("abc", "", -1));
"#,
    );
    assert_eq!(
        out,
        "int(7)\nbool(false)\nint(7)\nint(7)\nint(4)\nint(1)\nbool(false)\nint(3)\nint(2)\n"
    );
}

/// Verifies the `$offset` window is computed from values the optimizer cannot fold, so the
/// backend's own normalization, `ValueError` guard, and match rebasing are exercised rather
/// than a compile-time constant. `$argc` is 1 for a binary run without arguments.
/// Expected values are verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_string_position_offset_from_runtime_values() {
    let out = compile_and_run(
        r#"<?php
$haystack = "abcabc" . ($argc > 100 ? "z" : "");
$needle = "bc";
var_dump(strpos($haystack, $needle, $argc + 1));
var_dump(strrpos($haystack, $needle, -$argc - 2));
var_dump(strrpos($haystack, $needle, offset: -$argc - 5));
"#,
    );
    assert_eq!(out, "int(4)\nint(1)\nbool(false)\n");
}

/// Verifies both position builtins raise php-src's catchable `ValueError` for an `$offset`
/// that does not land inside the haystack, in either direction.
/// Messages are verbatim `LC_ALL=C php` 8.4 output.
#[test]
fn test_string_position_offset_out_of_range_value_errors() {
    let out = compile_and_run(
        r#"<?php
try { strpos("abc", "a", 4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { strpos("abc", "a", -4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { strrpos("abc", "a", 4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { strrpos("abc", "a", -4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
strrpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
strrpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n"
    );
}

/// Verifies `stripos()` finds the FIRST case-insensitive occurrence of a needle.
///
/// Folding is ASCII-only, matching php-src's locale-independent `zend_tolower_ascii`: the
/// bracket/brace case checks that the byte range just outside `A`-`Z` is compared verbatim,
/// and `stripos("Été", "é")` is 3 rather than 1 because `0x89` and `0xA9` do not fold onto
/// each other. Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_stripos_finds_first_case_insensitive_match() {
    let out = compile_and_run(
        r#"<?php
var_dump(stripos("Hello World", "WORLD"));
var_dump(stripos("Hello World", "world"));
var_dump(stripos("ABCabc", "abc"));
var_dump(stripos("Hello World", "zz"));
var_dump(stripos("Hello World", ""));
var_dump(stripos("[]{}", "{"));
var_dump(stripos("\xC3\x89t\xC3\xA9", "\xC3\xA9"));
"#,
    );
    assert_eq!(
        out,
        "int(6)\nint(6)\nint(0)\nbool(false)\nint(0)\nint(2)\nint(3)\n"
    );
}

/// Verifies `strripos()` finds the LAST case-insensitive occurrence of a needle.
///
/// The overlapping `strripos("aAaA", "aa")` case pins the right-to-left scan: a left-to-right
/// search would answer 0. An empty needle answers the haystack length, like `strrpos()`.
/// Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_strripos_finds_last_case_insensitive_match() {
    let out = compile_and_run(
        r#"<?php
var_dump(strripos("Hello World", "O"));
var_dump(strripos("ABCabc", "ABC"));
var_dump(strripos("aAaA", "aa"));
var_dump(strripos("Hello World", "zz"));
var_dump(strripos("Hello World", ""));
"#,
    );
    assert_eq!(out, "int(7)\nint(3)\nint(2)\nbool(false)\nint(11)\n");
}

/// Verifies `stripos()`/`strripos()` accept PHP's third `$offset` argument positionally and
/// by name, with the same direction-dependent negative-offset rules as `strpos()`/`strrpos()`.
/// Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_case_insensitive_position_offset_positional_and_named() {
    let out = compile_and_run(
        r#"<?php
var_dump(stripos("Hello World", "O", 5));
var_dump(stripos("Hello World", "O", -4));
var_dump(stripos("Hello World", "L", offset: 4));
var_dump(stripos("aAaA", "aa", 1));
var_dump(strripos("Hello World", "O", 5));
var_dump(strripos("aAaA", "aa", -2));
var_dump(strripos("ABCabc", "ABC", offset: 1));
var_dump(stripos("abc", "B", 3));
var_dump(strripos("abc", "B", -3));
"#,
    );
    assert_eq!(
        out,
        "int(7)\nint(7)\nint(9)\nint(1)\nint(7)\nint(2)\nint(3)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies the case-insensitive `$offset` window is computed from values the optimizer cannot
/// fold, so the backend's own normalization, `ValueError` guard, and match rebasing run rather
/// than a compile-time constant. `$argc` is 1 for a binary run without arguments.
/// Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_case_insensitive_position_offset_from_runtime_values() {
    let out = compile_and_run(
        r#"<?php
$haystack = "aBcaBc" . ($argc > 100 ? "z" : "");
$needle = "bC";
var_dump(stripos($haystack, $needle, $argc + 1));
var_dump(strripos($haystack, $needle, -$argc - 2));
var_dump(strripos($haystack, $needle, offset: -$argc - 5));
"#,
    );
    assert_eq!(out, "int(4)\nint(1)\nbool(false)\n");
}

/// Verifies both case-insensitive position builtins raise php-src's catchable `ValueError`
/// for an `$offset` that does not land inside the haystack, in either direction.
/// Messages are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_case_insensitive_position_offset_out_of_range_value_errors() {
    let out = compile_and_run(
        r#"<?php
try { stripos("abc", "a", 4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { stripos("abc", "a", -4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { strripos("abc", "a", 4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { strripos("abc", "a", -4); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "stripos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
stripos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
strripos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n\
strripos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\n"
    );
}

/// Verifies `stripos()`/`strripos()` through case-insensitive, namespaced, and dynamic call
/// sites, so the registry catalog resolves all three spellings to the same runtime target.
#[test]
fn test_case_insensitive_position_case_insensitive_and_namespaced() {
    let out = compile_and_run(
        r#"<?php
namespace App;
var_dump(\STRIPOS("Hello World", "WORLD"));
var_dump(StrRiPos("Hello World", "o"));
var_dump(call_user_func('stripos', 'FooBar', 'BAR'));
"#,
    );
    assert_eq!(out, "int(6)\nint(7)\nint(3)\n");
}

/// `strtok()` has no backend implementation of its own: the prelude keeps the string and the offset
/// in `static` locals. Expected output is `php -n` 8.5's, byte for byte.
#[test]
fn test_strtok_tokenizes_like_php() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$s = "This is\tan example\nstring";
$t = strtok($s, " \n\t");
while ($t !== false) {
    echo "[", $t, "]";
    $t = strtok(" \n\t");
}
echo "\n";
$t = strtok("//a//b///c/", "/");
echo $t, strtok("/"), strtok("/"), "|", var_export(strtok("/"), true), "|", var_export(strtok("/"), true), "\n";
echo var_export(strtok("abc", ""), true), var_export(strtok(""), true), "\n";
echo var_export(strtok("", "x"), true), var_export(strtok("x"), true), "\n";
echo strtok("one two three", " "), "-", strtok(" "), "-";
echo strtok("alpha,beta;gamma", ",;"), "-", strtok(",;"), "-", strtok(",;"), "\n";
function col(string $type): string { return strtok($type, '(') ?: ''; }
echo col("varchar(255)"), "|", col("integer"), "|", col("(x)"), "|", col(""), "\n";
echo var_export(strtok("a\0b\0c", "\0"), true), var_export(strtok("\0"), true), bin2hex((string) strtok("\0")), "\n";
$parts = [];
for ($tok = strtok("k1=v1&k2=v2", "&="); $tok !== false; $tok = strtok("&=")) {
    $parts[] = $tok;
}
echo implode(",", $parts), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "[This][is][an][example][string]\nabc|false|false\n'abc'false\nfalsefalse\none-two-alpha-beta-gamma\nvarchar|integer|x)|\n'a''b'63\nk1,v1,k2,v2\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Interpreted code reaches its own `strtok()` (Laravel's SQLite processor calls it), with the
/// same semantics and a request-scoped state of its own.
#[test]
fn test_eval_strtok_tokenizes_like_php() {
    let output = compile_and_run(
        r#"<?php
eval(<<<'EOT'
$s = "This is\tan example\nstring";
$t = strtok($s, " \n\t");
while ($t !== false) {
    echo "[", $t, "]";
    $t = strtok(" \n\t");
}
echo "\n";
$t = strtok("//a//b///c/", "/");
echo $t, strtok("/"), strtok("/"), "|", var_export(strtok("/"), true), "|", var_export(strtok("/"), true), "\n";
echo var_export(strtok("abc", ""), true), var_export(strtok(""), true), "\n";
echo var_export(strtok("", "x"), true), var_export(strtok("x"), true), "\n";
echo strtok("one two three", " "), "-", strtok(" "), "-";
echo strtok("alpha,beta;gamma", ",;"), "-", strtok(",;"), "-", strtok(",;"), "\n";
function col(string $type): string { return strtok($type, '(') ?: ''; }
echo col("varchar(255)"), "|", col("integer"), "|", col("(x)"), "|", col(""), "\n";
echo var_export(strtok("a\0b\0c", "\0"), true), var_export(strtok("\0"), true), bin2hex((string) strtok("\0")), "\n";
$parts = [];
for ($tok = strtok("k1=v1&k2=v2", "&="); $tok !== false; $tok = strtok("&=")) {
    $parts[] = $tok;
}
echo implode(",", $parts), "\n";

EOT
);
"#,
    );
    assert_eq!(output, "[This][is][an][example][string]\nabc|false|false\n'abc'false\nfalsefalse\none-two-alpha-beta-gamma\nvarchar|integer|x)|\n'a''b'63\nk1,v1,k2,v2\n");
}
