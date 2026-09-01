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

/// Verifies `str_repeat()` applies PHP's weak scalar-to-string coercion to its first argument.
#[test]
fn test_str_repeat_coerces_integer_source_to_string() {
    let out = compile_and_run(r#"<?php echo str_repeat(7, 3);"#);
    assert_eq!(out, "777");
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

/// Verifies an uncaught `str_repeat()` negative count reports PHP's uncaught-`ValueError` fatal.
///
/// The count used to reach `__rt_str_repeat`, which printed a bare fatal no `catch` block could
/// ever intercept. It is now screened at the call site and raised as a real `ValueError`, so the
/// uncaught diagnostic gains PHP's `Uncaught ValueError:` prefix.
#[test]
fn test_str_repeat_negative_count_reports_runtime_error() {
    let err = compile_and_run_expect_failure(r#"<?php echo str_repeat("ab", -1);"#);
    assert!(err.contains(
        "Fatal error: Uncaught ValueError: str_repeat(): Argument #2 ($times) must be greater than or equal to 0"
    ));
}

/// Verifies `str_repeat()` with a negative count raises a catchable `ValueError` like PHP 8.4.
#[test]
fn test_str_repeat_negative_count_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    echo str_repeat("ab", -1);
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|", str_repeat("ab", 0), "|", str_repeat("ab", 2);
"#,
    );
    assert_eq!(
        out,
        "ValueError|str_repeat(): Argument #2 ($times) must be greater than or equal to 0||abab"
    );
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

/// Verifies `implode()` and `join()` accept an ASSOCIATIVE array, joining its values.
///
/// PHP ignores the keys entirely, so this is ordinary code — but the renderers walk a dense
/// indexed payload and a hash was rejected outright at lowering time, whatever its value
/// type. The sparse-key case is the one that shows the keys really are ignored rather than
/// used as positions.
#[test]
fn test_implode_joins_associative_array_values() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", ["a" => 1, "b" => 2, "c" => 3]), "|";
echo implode("-", ["x" => "p", "y" => "q"]), "|";
echo join(["x" => "p", "y" => "q"]), "|";
echo implode("|", [0 => "u", 5 => "v", 9 => "w"]);
"#,
    );
    assert_eq!(out, "1,2,3|p-q|pq|u|v|w");
}

/// Verifies the indexed copy `implode()` makes from a hash is released.
///
/// The values are copied into a fresh indexed array the caller owns, so the join has to
/// stack its STRING result pair while that copy is released — releasing first would free the
/// payload the join just read. A single call hides an imbalance; the loop is what makes one
/// accumulate. Measured with the release removed: 7 blocks and 400 bytes leaked.
#[test]
fn test_implode_releases_the_indexed_copy_it_makes_from_a_hash() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$map = ["a" => 1, "b" => 2, "c" => 3];
$strs = ["x" => "p", "y" => "q"];
for ($i = 0; $i < 200; $i++) {
    $s = implode(",", $map);
    $t = implode("-", $strs);
}
echo $s, $t;
"#,
    );
    let report = format!("{}{}", out.stdout, out.stderr);
    assert!(
        report.contains("leak summary: clean"),
        "implode must release the indexed copy it makes from a hash:\n{report}"
    );
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

/// Verifies `str_pad()` with an empty `$pad_string` raises PHP's catchable `ValueError`.
///
/// The runtime pad loop copied `$length - strlen($string)` bytes out of the pad string, so an
/// empty pad string made it read whatever followed the zero-length buffer: `str_pad("x", 4, "")`
/// returned `"xUUU"` built from uninitialized memory. PHP only rejects the empty pad string when
/// padding would actually happen, so the shorter-`$length` call must still return the input.
#[test]
fn test_str_pad_empty_pad_string_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    echo str_pad("x", 4, "");
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|", str_pad("xyz", 1, ""), "|", str_pad("x", 4, "-", STR_PAD_LEFT);
"#,
    );
    assert_eq!(
        out,
        "ValueError|str_pad(): Argument #3 ($pad_string) must not be empty|xyz|---x"
    );
}

/// Verifies an uncaught empty `str_pad()` pad string reports PHP's uncaught-`ValueError` fatal.
#[test]
fn test_str_pad_empty_pad_string_uncaught_reports_value_error_fatal() {
    let err = compile_and_run_expect_failure(r#"<?php echo str_pad("x", 4, "");"#);
    assert!(err.contains(
        "Fatal error: Uncaught ValueError: str_pad(): Argument #3 ($pad_string) must not be empty"
    ));
}

/// Verifies `str_pad()` rejects a `$pad_type` outside `STR_PAD_LEFT`/`RIGHT`/`BOTH`.
#[test]
fn test_str_pad_invalid_pad_type_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    echo str_pad("x", 4, "ab", 9);
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|", str_pad("x", 5, "ab", STR_PAD_BOTH);
"#,
    );
    assert_eq!(
        out,
        "ValueError|str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH|abxab"
    );
}

/// Verifies `str_split()` with a non-positive `$length` raises PHP's catchable `ValueError`.
///
/// `__rt_str_split` advanced its cursor by the chunk length, so `0` spun forever pushing empty
/// chunks until the heap was exhausted and `-1` walked the cursor backwards and crashed.
#[test]
fn test_str_split_non_positive_length_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
foreach ([0, -1] as $len) {
    try {
        var_dump(str_split("abc", $len));
    } catch (ValueError $e) {
        echo get_class($e), "|", $e->getMessage(), "\n";
    }
}
echo implode(",", str_split("abcde", 2)), "\n";
"#,
    );
    assert_eq!(
        out,
        "ValueError|str_split(): Argument #2 ($length) must be greater than 0\n\
         ValueError|str_split(): Argument #2 ($length) must be greater than 0\n\
         ab,cd,e\n"
    );
}

/// Verifies an uncaught zero `str_split()` chunk length reports PHP's uncaught-`ValueError` fatal.
#[test]
fn test_str_split_zero_length_uncaught_reports_value_error_fatal() {
    let err = compile_and_run_expect_failure(r#"<?php var_dump(str_split("abc", 0));"#);
    assert!(err.contains(
        "Fatal error: Uncaught ValueError: str_split(): Argument #2 ($length) must be greater than 0"
    ));
}

/// Verifies `explode()` with an empty separator raises PHP's catchable `ValueError`.
///
/// A zero-length separator matched at every position, so the splitter never advanced and pushed
/// empty segments until the heap was exhausted.
#[test]
fn test_explode_empty_separator_is_a_catchable_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    var_dump(explode("", "abc"));
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|", implode("/", explode(",", "a,b,c"));
"#,
    );
    assert_eq!(
        out,
        "ValueError|explode(): Argument #1 ($separator) must not be empty|a/b/c"
    );
}

/// Verifies an uncaught empty `explode()` separator reports PHP's uncaught-`ValueError` fatal.
#[test]
fn test_explode_empty_separator_uncaught_reports_value_error_fatal() {
    let err = compile_and_run_expect_failure(r#"<?php var_dump(explode("", "abc"));"#);
    assert!(err.contains(
        "Fatal error: Uncaught ValueError: explode(): Argument #1 ($separator) must not be empty"
    ));
}

/// Verifies `explode()`'s third `$limit` parameter follows php-src for every sign.
///
/// A positive limit caps the element count and lets the last element absorb the remaining
/// suffix, `0` behaves like `1`, and a negative limit drops that many trailing segments —
/// including down to an empty array when it drops them all.
#[test]
fn test_explode_limit_matches_php_for_every_sign() {
    let out = compile_and_run(
        r#"<?php
$s = "a,b,c";
echo implode("/", explode(",", $s, 0)), "|";
echo implode("/", explode(",", $s, 1)), "|";
echo implode("/", explode(",", $s, 2)), "|";
echo implode("/", explode(",", $s, 3)), "|";
echo implode("/", explode(",", $s, 99)), "|";
echo count(explode(",", $s, -1)), ":", implode("/", explode(",", $s, -1)), "|";
echo count(explode(",", $s, -2)), ":", implode("/", explode(",", $s, -2)), "|";
echo count(explode(",", $s, -3)), "|";
echo count(explode(",", $s, -9)), "|";
echo count(explode(",", "", -1)), "|";
echo count(explode(",", "", 0));
"#,
    );
    assert_eq!(out, "a,b,c|a,b,c|a/b,c|a/b/c|a/b/c|2:a/b|1:a|0|0|0|1");
}

/// Verifies `wordwrap()` rejects the argument combinations reference PHP refuses to wrap with.
///
/// An empty `$break` left the wrapper with nothing to insert, so it silently returned the input
/// unwrapped; a zero `$width` with `$cut_long_words` asks for progress-free cutting. php-src
/// checks `$break` first, then the width/cut pair.
#[test]
fn test_wordwrap_invalid_arguments_are_catchable_value_errors() {
    let out = compile_and_run(
        r#"<?php
try {
    echo wordwrap("ab cd", 3, "");
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|";
try {
    echo wordwrap("abcdef", 0, "\n", true);
} catch (ValueError $e) {
    echo get_class($e), "|", $e->getMessage();
}
echo "|", wordwrap("ab cd", 3, "|"), "|", wordwrap("abcdef", 0, "\n", false);
"#,
    );
    assert_eq!(
        out,
        "ValueError|wordwrap(): Argument #3 ($break) must not be empty|\
         ValueError|wordwrap(): Argument #4 ($cut_long_words) cannot be true when argument #2 ($width) is 0|\
         ab|cd|abcdef"
    );
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

/// Verifies `str_pad()` to a target width far beyond the 64 KiB concat scratch buffer produces the
/// full padded string instead of running the pad loop past the scratch end (overflow regression).
#[test]
fn test_str_pad_target_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$p = str_pad("a", 1 << 20, "xy");
echo strlen($p), "|", substr($p, 0, 5), "|", substr($p, -5);
"#,
    );
    assert_eq!(out, "1048576|axyxy|xyxyx");
}

/// Verifies `str_repeat()` reports PHP's allocation-overflow fatal error instead of crashing when
/// `length * times` wraps a machine word (`4 * 2^62` wraps to 0, which used to pass the 64 KiB
/// scratch check and then SIGBUS while copying `times * length` bytes).
#[test]
fn test_str_repeat_length_times_overflow_is_a_controlled_error() {
    let err = compile_and_run_expect_failure(r#"<?php echo str_repeat("aaaa", 4611686018427387904);"#);
    assert!(
        err.contains("Fatal error: Possible integer overflow in memory allocation"),
        "expected an allocation-overflow fatal error, got: {err}"
    );
}

// --- Expansive transformers beyond the 64 KiB concat scratch buffer ---

/// Verifies `htmlspecialchars()` on a payload whose worst-case 6x entity expansion cannot fit the
/// shared 64 KiB concat scratch buffer produces the full correct escaping instead of writing past
/// the scratch end into the adjacent BSS globals (concat-scratch overflow regression).
#[test]
fn test_htmlspecialchars_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$h = htmlspecialchars(str_repeat("<a href=\"x\">&'", 8000));
echo strlen($h), "|", substr($h, 0, 12), "|", substr($h, -12);
"#,
    );
    assert_eq!(out, "312000|&lt;a href=&|;&amp;&#039;");
}

/// Verifies `html_entity_decode()` of an entity-encoded payload longer than the 64 KiB concat
/// scratch buffer decodes every entity through the heap fallback.
#[test]
fn test_html_entity_decode_input_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$d = html_entity_decode(str_repeat("&lt;a&gt;&amp;&quot;&#039;", 5000));
echo strlen($d), "|", substr($d, 0, 8), "|", substr($d, -8);
"#,
    );
    assert_eq!(out, "30000|<a>&\"'<a|\"'<a>&\"'");
}

/// Verifies `nl2br()` on an all-newline-heavy payload whose worst-case 7x expansion exceeds the
/// 64 KiB concat scratch buffer keeps every injected break tag intact.
#[test]
fn test_nl2br_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$n = nl2br(str_repeat("a\nb\n", 20000));
echo strlen($n), "|", substr($n, 0, 8), "|", substr($n, -8);
"#,
    );
    assert_eq!(out, "320000|a<br />\n|b<br />\n");
}

/// Verifies `addslashes()` / `stripslashes()` round-trip a payload whose 2x escaped form exceeds
/// the 64 KiB concat scratch buffer, so both directions take the heap fallback and stay byte-exact.
#[test]
fn test_addslashes_roundtrip_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("a'b\"c\\d", 20000);
$a = addslashes($s);
$b = stripslashes($a);
echo strlen($a), "|", substr($a, 0, 10), "|", strlen($b), "|", ($b === $s ? "same" : "DIFF");
"#,
    );
    assert_eq!(out, "200000|a\\'b\\\"c\\\\d|140000|same");
}

/// Verifies `wordwrap()` on a payload whose wrapped form far exceeds the 64 KiB concat scratch
/// buffer inserts every break string instead of running the copy helper past the scratch end.
#[test]
fn test_wordwrap_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$t = str_repeat("hello world ", 8000);
$w = wordwrap($t, 5, "<BR>", true);
echo strlen($w), "|", substr($w, 0, 14), "|", substr($w, -9);
"#,
    );
    assert_eq!(out, "144000|hello<BR>world|world<BR>");
}

/// Verifies `str_replace()` with an expanding replacement whose result exceeds the 64 KiB concat
/// scratch buffer emits every replacement instead of overrunning the scratch end.
#[test]
fn test_str_replace_expansion_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("ab", 40000);
$r = str_replace("a", "XYZW", $s);
echo strlen($r), "|", substr($r, 0, 10), "|", substr($r, -10);
"#,
    );
    assert_eq!(out, "200000|XYZWbXYZWb|XYZWbXYZWb");
}

/// Verifies `str_ireplace()` with an expanding case-insensitive replacement whose result exceeds
/// the 64 KiB concat scratch buffer stays byte-exact through the heap fallback.
#[test]
fn test_str_ireplace_expansion_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("ab", 40000);
$i = str_ireplace("A", "0123456789", $s);
echo strlen($i), "|", substr($i, 0, 12), "|", substr($i, -12);
"#,
    );
    assert_eq!(out, "440000|0123456789b0|b0123456789b");
}

/// Verifies `substr_replace()` on a subject plus replacement that together exceed the 64 KiB
/// concat scratch buffer keeps the prefix, replacement, and suffix intact.
#[test]
fn test_substr_replace_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("x", 50000);
$r = str_repeat("R", 50000);
$o = substr_replace($s, $r, 100, 200);
echo strlen($o), "|", $o[0], $o[99], $o[100], $o[50099], $o[50100];
"#,
    );
    assert_eq!(out, "99800|xxRRx");
}

/// Verifies `number_format()` still formats correctly when the shared concat scratch buffer is
/// already nearly full, which used to spill its grouped output past the scratch end.
#[test]
fn test_number_format_near_end_of_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$pad = str_repeat("z", 65400);
$n = $pad . number_format(1234567.891, 2, ".", ",");
echo strlen($n), "|", substr($n, -12);
"#,
    );
    assert_eq!(out, "65412|1,234,567.89");
}

/// Verifies `join()` behaves as `implode()`'s alias for the two-argument call form.
/// Fixture and expectation come from `LC_ALL=C php`: `join(", ", ["a","b","c"])` is `"a, b, c"`.
#[test]
fn test_join_with_separator() {
    let out = compile_and_run(r#"<?php echo join(", ", ["a", "b", "c"]);"#);
    assert_eq!(out, "a, b, c");
}

/// Verifies `join()`'s single-argument form joins with an empty separator.
/// PHP declares `join(string|array $separator, ?array $array = null)`, so `join($array)`
/// concatenates the elements: `LC_ALL=C php` prints `abc`.
#[test]
fn test_join_single_array_argument() {
    let out = compile_and_run(r#"<?php echo join(["a", "b", "c"]);"#);
    assert_eq!(out, "abc");
}

/// Verifies `join()` resolves case-insensitively and through a namespace-qualified call,
/// and that its parameters are reachable by name.
#[test]
fn test_join_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php
$a = ["x", "y"];
echo JOIN("-", $a), "|", \join("+", $a), "|", join(separator: "*", array: $a);
"#,
    );
    assert_eq!(out, "x-y|x+y|x*y");
}

/// Verifies `join()` joins an integer array, which routes through the integer renderer.
#[test]
fn test_join_int_array() {
    let out = compile_and_run(r#"<?php echo join("+", [1, 2, 3]), "|", join([1, 2, 3]);"#);
    assert_eq!(out, "1+2+3|123");
}

/// Verifies `implode()`/`join()` accept an empty array literal, whose element type is
/// uninhabited. `LC_ALL=C php` prints an empty string for both.
#[test]
fn test_join_empty_array() {
    let out = compile_and_run(r#"<?php echo "[", implode("", []), join([]), "]";"#);
    assert_eq!(out, "[]");
}

/// Verifies `ucwords()` accepts PHP's second `$separators` argument positionally and by
/// name. `$separators` is a byte SET: each listed byte ends a word, an empty set leaves only
/// the very first character capitalized, and a separator run capitalizes only the byte after
/// the last one. The final case pins PHP's own default set, which includes the `\r`, `\f`,
/// and `\v` that the previous hard-coded space/tab/newline scan silently ignored.
/// Every expected value is verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_ucwords_separators_positional_and_named() {
    let out = compile_and_run(
        r#"<?php
var_dump(ucwords("hello world"));
var_dump(ucwords("hello|world", "|"));
var_dump(ucwords("hello-world", "-"));
var_dump(ucwords("hello world-again", " -"));
var_dump(ucwords("hello world", ""));
var_dump(ucwords("", "-"));
var_dump(ucwords("a.b.c", "."));
var_dump(ucwords("hello world", separators: "|"));
var_dump(ucwords(string: "hello|world", separators: "|"));
var_dump(ucwords("--x", "-"));
var_dump(ucwords("1abc def"));
var_dump(ucwords("HELLO world"));
var_dump(ucwords("hello\tworld\nfoo\rbar\fbaz\vqux"));
"#,
    );
    assert_eq!(
        out,
        "string(11) \"Hello World\"\n\
string(11) \"Hello|World\"\n\
string(11) \"Hello-World\"\n\
string(17) \"Hello World-Again\"\n\
string(11) \"Hello world\"\n\
string(0) \"\"\n\
string(5) \"A.B.C\"\n\
string(11) \"Hello world\"\n\
string(11) \"Hello|World\"\n\
string(3) \"--X\"\n\
string(8) \"1abc Def\"\n\
string(11) \"HELLO World\"\n\
string(27) \"Hello\tWorld\nFoo\rBar\u{0c}Baz\u{0b}Qux\"\n"
    );
}

/// Verifies the separator set is honored when neither the subject nor the set can be folded
/// at compile time, so the runtime membership scan is exercised rather than a constant.
/// `$argc` is 1 for a binary run without arguments.
/// Expected values are verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_ucwords_separators_from_runtime_values() {
    let out = compile_and_run(
        r#"<?php
$subject = "hello|world-again" . ($argc > 100 ? "z" : "");
$separators = "|" . ($argc > 100 ? "" : "-");
var_dump(ucwords($subject, $separators));
var_dump(ucwords($subject, separators: $separators));
var_dump(ucwords($subject));
"#,
    );
    assert_eq!(
        out,
        "string(17) \"Hello|World-Again\"\n\
string(17) \"Hello|World-Again\"\n\
string(17) \"Hello|world-again\"\n"
    );
}
