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
