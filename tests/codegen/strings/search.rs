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
/// Verifies sprintf's `%b` conversion and `%'X` pad character, both php-only.
#[test]
fn test_sprintf_binary_and_pad_character() {
    // Neither exists in C, so libc echoed the letters back: "%b" printed "b" and
    // "%'*10d" printed "*10d". %b expands the value as unsigned 64-bit -- %b of -1
    // is sixty-four ones -- and a precision empties it, as it does for %x and %o.
    let binary = compile_and_run(
        r#"<?php
echo "[", sprintf("%b", 5), "]";
echo "[", sprintf("%b", 0), "]";
echo "[", sprintf("%b", 255), "]";
echo "[", sprintf("%b", -1), "]";
echo "[", sprintf("%10b", 5), "]";
echo "[", sprintf("%-10b", 5), "]";
echo "[", sprintf("%010b", 5), "]";
echo "[", sprintf("%.3b", 5), "]";
echo "[", sprintf("%b", 3.9), "]";
"#,
    );
    assert_eq!(
        binary,
        "[101][0][11111111][1111111111111111111111111111111111111111111111111111111111111111][       101][101       ][0000000101][][11]"
    );

    // The byte after the quote is always the pad character, even when it is '-' or
    // a digit, and it composes with left alignment and with every conversion. A
    // quoted '0' is the zero flag, which pads after the sign rather than uniformly.
    let padded = compile_and_run(
        r#"<?php
echo "[", sprintf("%'x8s", "ab"), "]";
echo "[", sprintf("%'-8s", "ab"), "]";
echo "[", sprintf("%'x-8s", "ab"), "]";
echo "[", sprintf("%'*10d", 42), "]";
echo "[", sprintf("%'*10d", -42), "]";
echo "[", sprintf("%'*-10d", 42), "]";
echo "[", sprintf("%'x8.2f", 1.5), "]";
echo "[", sprintf("%'*6x", 255), "]";
echo "[", sprintf("%'*10b", 5), "]";
echo "[", sprintf("%'08d", -42), "]";
"#,
    );
    assert_eq!(
        padded,
        "[xxxxxxab][------ab][abxxxxxx][********42][*******-42][42********][xxxx1.50][****ff][*******101][-0000042]"
    );
}

/// Verifies sprintf honours php's `%N$` positional argument numbers.
#[test]
fn test_sprintf_positional_arguments() {
    // php lets a specifier name its argument, and a named one does not advance the
    // sequential counter: "%s|%2$s|%s" reads arguments 0, 1 and 1, not 0, 1 and 2.
    // Two layers were missing. The runtime scanner read "1$" as a width followed by
    // an unknown conversion, and the compile-time parser that decides how each
    // argument is packed took the '$' for the conversion character -- so "%1$s"
    // packed its string argument as an integer and it reached the runtime with a
    // garbage length. An integer argument happened to survive that, which is why
    // "%2$05d" appeared to work.
    let out = compile_and_run(
        r#"<?php
echo "[", sprintf('%2$s-%1$s', "a", "b"), "]";
echo "[", sprintf('%1$s%1$s', "x"), "]";
echo "[", sprintf('%s|%2$s|%s', "a", "b", "c"), "]";
echo "[", sprintf('%2$05d', 1, 42), "]";
echo "[", sprintf('%10$s', "a","b","c","d","e","f","g","h","i","j"), "]";
echo "[", sprintf('%2$s %1$d', 7, "hi"), "]";
echo "[", sprintf('%1$s-%2$s-%1$s', "p", "q"), "]";
"#,
    );
    assert_eq!(out, "[b-a][xx][a|b|b][00042][j][hi 7][p-q-p]");

    // Digits that are not followed by '$' are still a width, and a runtime format
    // string takes the same path.
    let widths = compile_and_run(
        r#"<?php
echo "[", sprintf("%5d", 42), "]";
echo "[", sprintf("%2s", "ab"), "]";
echo "[", sprintf("%s%s", "a", "b"), "]";
$f = '%2$s-%1$s';
echo "[", sprintf($f, "a", "b"), "]";
"#,
    );
    assert_eq!(widths, "[   42][ab][ab][b-a]");
}

/// Verifies a conversion wider than the scratch buffer emits its own bytes.
#[test]
fn test_sprintf_wide_conversion_does_not_emit_stack_memory() {
    // snprintf reports the length it *would* have written, not what it wrote. The
    // copy loop trusted that count against a 128-byte buffer, so anything wider
    // emitted whatever sat next to it on the stack: sprintf("%200d", 5) came back
    // 200 bytes long -- which is why comparing lengths hides this -- but with the
    // wrong content and a leak of adjacent memory. An oversized result is now
    // re-rendered straight into the destination.
    let out = compile_and_run(
        r#"<?php
echo strlen(sprintf("%127d", 5)), ":", substr(md5(sprintf("%127d", 5)), 0, 8), " ";
echo strlen(sprintf("%128d", 5)), ":", substr(md5(sprintf("%128d", 5)), 0, 8), " ";
echo strlen(sprintf("%200d", 5)), ":", substr(md5(sprintf("%200d", 5)), 0, 8), " ";
echo strlen(sprintf("%200x", 255)), ":", substr(md5(sprintf("%200x", 255)), 0, 8), " ";
echo strlen(sprintf("%400.2f", 1.5)), ":", substr(md5(sprintf("%400.2f", 1.5)), 0, 8);
"#,
    );
    assert_eq!(
        out,
        "127:b141e625 128:5731c77f 200:cb6534c0 200:e6d4e9e6 400:6289ecc2"
    );
}

/// Verifies sprintf applies php's precision and space-flag rules, not C's.
#[test]
fn test_sprintf_precision_follows_php_not_c() {
    // php does not implement C's formatter. Precision means a different thing per
    // conversion: %d, %u and %c ignore it, %x, %X and %o render nothing at all, %s
    // truncates and %f counts digits. The space flag, which reserves a sign column
    // in C, carries no meaning at all. The mini format handed to snprintf was
    // passing all of these straight through, so nine cases disagreed with php.
    let out = compile_and_run(
        r#"<?php
echo "[", sprintf("%.5d", 42), "]";
echo "[", sprintf("%.0d", 0), "]";
echo "[", sprintf("%05.3d", 42), "]";
echo "[", sprintf("%.3u", 42), "]";
echo "[", sprintf("%.2x", 255), "]";
echo "[", sprintf("%5.2x", 255), "]";
echo "[", sprintf("%.2o", 8), "]";
echo "[", sprintf("%.2c", 65), "]";
echo "[", sprintf("% 05d", 42), "]";
echo "[", sprintf("% f", 1.5), "]";
echo "[", sprintf("% d", -42), "]";
"#,
    );
    assert_eq!(out, "[42][0][00042][42][][     ][][A][00042][1.500000][-42]");

    // The conversions php and C do agree on must keep working: the zero pad lands
    // after the sign, precision still counts digits on floats, and %e drops the
    // leading zero C puts in the exponent.
    let unchanged = compile_and_run(
        r#"<?php
echo "[", sprintf("%05d", -42), "]";
echo "[", sprintf("%+05d", -42), "]";
echo "[", sprintf("%08.2f", -1.5), "]";
echo "[", sprintf("%-10.2f", 3.5), "]";
echo "[", sprintf("%u", -1), "]";
echo "[", sprintf("%e", 42.0), "]";
"#,
    );
    assert_eq!(
        unchanged,
        "[-0042][-0042][-0001.50][3.50      ][18446744073709551615][4.200000e+1]"
    );
}

/// Verifies every integer conversion formats the full 64 bits of a php integer.
#[test]
fn test_sprintf_integer_conversions_are_64_bit() {
    // php integers are 64-bit, so the mini format handed to snprintf has to name the
    // "ll" length modifier. The x86_64 arm never wrote it, so the C formatter read
    // only the low 32 bits and every value whose top half mattered came back wrong:
    // sprintf("%u", -1) rendered 4294967295 instead of 18446744073709551615. The
    // AArch64 arm has always written it, which is why no fixture had caught this --
    // and why the assertions below have to use values above 2**32 to mean anything.
    let out = compile_and_run(
        r#"<?php
echo "[", sprintf("%d", PHP_INT_MAX), "]";
echo "[", sprintf("%d", PHP_INT_MIN), "]";
echo "[", sprintf("%x", PHP_INT_MAX), "]";
echo "[", sprintf("%X", -1), "]";
echo "[", sprintf("%o", PHP_INT_MAX), "]";
echo "[", sprintf("%u", -1), "]";
echo "[", sprintf("%u", PHP_INT_MIN), "]";
"#,
    );
    assert_eq!(
        out,
        "[9223372036854775807][-9223372036854775808][7fffffffffffffff]\
[FFFFFFFFFFFFFFFF][777777777777777777777][18446744073709551615]\
[9223372036854775808]"
    );
}

/// Verifies `sprintf("%s")` keeps every byte of its argument.
#[test]
fn test_sprintf_string_is_binary_safe_and_unbounded() {
    // The old path copied the argument into a 128-byte stack buffer purely to
    // NUL-terminate it for snprintf, clamping to 127 bytes. Every string longer than
    // that was silently truncated -- sprintf("%s", $json) lost data -- and the copy
    // stopped at the first NUL byte, which php strings are allowed to contain.
    let long = compile_and_run(
        r#"<?php
$long = str_repeat("ab", 100);
$out = sprintf("[%s]", $long);
echo strlen($out), "|", md5($out);
"#,
    );
    assert_eq!(long, "202|e54a09c7a5a192f97c78551bb5a06799");

    let embedded_nul = compile_and_run(
        r#"<?php
$out = sprintf("[%s]", "ab\0cd");
echo strlen($out), "|", md5($out);
"#,
    );
    assert_eq!(embedded_nul, "7|a6fd15fcb18f7db1fafb1c111ed521a0");

    // php's %s is truncate-to-precision then pad; '+' carries no meaning on strings.
    let fields = compile_and_run(
        r#"<?php
echo "[", sprintf("%5.2s", "hello"), "]";
echo "[", sprintf("%-8s", "ab"), "]";
echo "[", sprintf("%08s", "abc"), "]";
echo "[", sprintf("%+s", "abcdef"), "]";
echo "[", sprintf("%3s", "abcdef"), "]";
echo "[", sprintf("%.0s", "abc"), "]";
"#,
    );
    assert_eq!(fields, "[   he][ab      ][00000abc][abcdef][abcdef][]");
}

/// Verifies `substr_replace()` reads a negative length as bytes kept at the end.
#[test]
fn test_substr_replace_negative_length() {
    // Same root cause as substr(): -1 doubled as the "no length argument" sentinel
    // and other negative lengths were clamped to zero, so the replacement swallowed
    // the tail php keeps. The clamp now runs against the available bytes rather than
    // through end = offset + length, which the i64::MAX sentinel would overflow.
    let out = compile_and_run(
        r#"<?php
echo substr_replace("hello", "X", 1, -1), "|";
echo substr_replace("hello", "X", 0, -2), "|";
echo substr_replace("hello", "X", 1, -9), "|";
echo substr_replace("hello", "X", 1), "|";
echo substr_replace("hello", "X", 1, 2), "|";
echo substr_replace("hello", "X", -3, -1), "|";
echo substr_replace("hello", "X", 1, 0);
"#,
    );
    assert_eq!(out, "hXo|Xlo|hXello|hX|hXlo|heXo|hXello");
}

/// Verifies `substr()` reads a negative length as bytes omitted from the end.
#[test]
fn test_substr_negative_length() {
    // php treats a negative length as "omit that many bytes from the end of the
    // string", so substr("hello", 1, -1) is "ell". Two faults compounded here: -1
    // doubled as the sentinel for "no length argument", making that call
    // indistinguishable from a two-argument one, and any other negative length was
    // clamped to zero, returning "" where php returns a prefix.
    let out = compile_and_run(
        r#"<?php
echo substr("hello", 1, -1), "|";
echo substr("hello", 0, -2), "|";
echo substr("hello", -4, -1), "|";
echo "[", substr("hello", 2, -5), "]|";
echo "[", substr("hello", 0, -9), "]|";
echo substr("hello", 1, 3), "|";
echo substr("hello", 1);
"#,
    );
    assert_eq!(out, "ell|hel|ell|[]|[]|ell|ello");
}

#[test]
fn test_substr_negative_offset() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", -5);"#);
    assert_eq!(out, "World");
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
