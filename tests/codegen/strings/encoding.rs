//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings encoding, including ord, ord empty string, and chr.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `ord()` returns the ASCII code 65 for a single uppercase "A".
#[test]
fn test_ord() {
    let out = compile_and_run(r#"<?php echo ord("A");"#);
    assert_eq!(out, "65");
}

/// Verifies `ord()` returns 0 for an empty string, matching PHP behavior.
#[test]
fn test_ord_empty_string() {
    let out = compile_and_run(r#"<?php echo ord("");"#);
    assert_eq!(out, "0");
}

/// Verifies `ord()` correctly handles double-quoted control character escapes:
/// \r (carriage return = 13), \v (vertical tab = 11), \e (escape = 27), \f (form feed = 12).
#[test]
fn test_double_quoted_control_escape_ord_values() {
    let out = compile_and_run(
        r#"<?php echo ord("\r") . "," . ord("\v") . "," . ord("\e") . "," . ord("\f");"#,
    );
    assert_eq!(out, "13,11,27,12");
}

/// Verifies double-quoted string escape handling: null byte (\x00), octal (\101 = 'A'),
/// Unicode grapheme (\u{1F600} = 😀), and that `strlen` and `ord` operate on the
/// actual byte representation after escape substitution.
#[test]
fn test_double_quoted_hex_octal_unicode_and_null_escapes() {
    let out = compile_and_run(
        r#"<?php
$s = "a\x00b";
echo "\x41\101\u{1F600}:" . strlen($s) . ":" . ord($s[1]);
"#,
    );
    assert_eq!(out, "AA😀:3:0");
}

/// Verifies high-byte escape sequences in double-quoted strings remain single PHP bytes:
/// \xFF (255), \777 octal (255), and Unicode scalar values outside BMP that encode as
/// multi-byte UTF-8 (\u{D800} → eda080, \u{E000} → ee8080).
#[test]
fn test_double_quoted_high_byte_escapes_remain_single_php_bytes() {
    let out = compile_and_run(
        r#"<?php
echo ord("\xFF") . ":" . ord("\777") . ":" . bin2hex("\xC3\xA9") . ":" . bin2hex("\u{D800}") . ":" . bin2hex("\u{E000}");
"#,
    );
    assert_eq!(out, "255:255:c3a9:eda080:ee8080");
}

/// Verifies `chr()` returns the single-character string "A" for ASCII code 65.
#[test]
fn test_chr() {
    let out = compile_and_run("<?php echo chr(65);");
    assert_eq!(out, "A");
}

/// Verifies `addslashes()` escapes double quotes and apostrophes with backslashes.
#[test]
fn test_addslashes() {
    let out = compile_and_run(r#"<?php echo addslashes("He said \"hi\" and it's ok");"#);
    assert_eq!(out, r#"He said \"hi\" and it\'s ok"#);
}

/// Verifies `stripslashes()` removes backslash escaping from \" and \' sequences.
#[test]
fn test_stripslashes() {
    let out = compile_and_run(r#"<?php echo stripslashes("He said \\\"hi\\\"");"#);
    assert_eq!(out, r#"He said "hi""#);
}

/// Verifies `nl2br()` inserts `<br />` before each newline while preserving the original \n.
#[test]
fn test_nl2br() {
    let out = compile_and_run("<?php echo nl2br(\"line1\\nline2\");");
    assert_eq!(out, "line1<br />\nline2");
}

/// Verifies `wordwrap()` breaks at word boundaries (the last space at/after the width),
/// matching PHP exactly rather than breaking mid-word at fixed column offsets.
#[test]
fn test_wordwrap() {
    let out = compile_and_run(
        r#"<?php echo wordwrap("The quick brown fox jumped over the lazy dog", 15, "\n");"#,
    );
    assert_eq!(out, "The quick brown\nfox jumped over\nthe lazy dog");
}

/// Verifies `wordwrap()` with the default cut flag leaves a word longer than the width intact
/// (PHP only breaks at spaces unless cut_long_words is set).
#[test]
fn test_wordwrap_long_word_not_cut() {
    let out = compile_and_run(r#"<?php echo wordwrap("A verylongword here", 8, "\n");"#);
    assert_eq!(out, "A\nverylongword\nhere");
}

/// Verifies `wordwrap()` with cut_long_words=true breaks an over-long word at the width.
#[test]
fn test_wordwrap_long_word_cut() {
    let out = compile_and_run(r#"<?php echo wordwrap("A verylongword here", 8, "\n", true);"#);
    assert_eq!(out, "A\nverylong\nword\nhere");
}

/// Verifies `wordwrap()` with cut_long_words=true chops a single space-free run into width-sized
/// pieces (no trailing break on the final short piece).
#[test]
fn test_wordwrap_cut_single_run() {
    let out = compile_and_run(r#"<?php echo wordwrap("abcdefghij", 4, "\n", true);"#);
    assert_eq!(out, "abcd\nefgh\nij");
}

/// Verifies `wordwrap()` with cut_long_words=false (default) returns a space-free over-long word
/// unchanged.
#[test]
fn test_wordwrap_no_cut_single_run() {
    let out = compile_and_run(r#"<?php echo wordwrap("abcdefghij", 4, "\n");"#);
    assert_eq!(out, "abcdefghij");
}

/// Verifies `wordwrap()` preserves existing newlines in the input and resets the line length at
/// each one (a hard break does not count toward the next line's width).
#[test]
fn test_wordwrap_preserves_existing_newlines() {
    let out = compile_and_run("<?php echo wordwrap(\"preserve\nnewlines here ok\", 10, \"\\n\");");
    assert_eq!(out, "preserve\nnewlines\nhere ok");
}

/// Verifies `wordwrap()` accepts a multi-character break string and inserts it at each wrap point.
#[test]
fn test_wordwrap_multichar_break() {
    let out = compile_and_run(r#"<?php echo wordwrap("aaa bbb ccc", 3, "<br>");"#);
    assert_eq!(out, "aaa<br>bbb<br>ccc");
}

/// Verifies `wordwrap()` leaves a string shorter than the width untouched (no break inserted).
#[test]
fn test_wordwrap_under_width_unchanged() {
    let out = compile_and_run(r#"<?php echo wordwrap("hello world", 20);"#);
    assert_eq!(out, "hello world");
}

/// Verifies `bin2hex()` converts a binary string "AB" to its hexadecimal representation "4142".
#[test]
fn test_bin2hex() {
    let out = compile_and_run(r#"<?php echo bin2hex("AB");"#);
    assert_eq!(out, "4142");
}

/// Verifies `hex2bin()` converts a hexadecimal string "4142" to the binary string "AB".
#[test]
fn test_hex2bin() {
    let out = compile_and_run(r#"<?php echo hex2bin("4142");"#);
    assert_eq!(out, "AB");
}

/// Verifies a roundtrip: `hex2bin(bin2hex("Hello"))` recovers the original string.
#[test]
fn test_bin2hex_hex2bin_roundtrip() {
    let out = compile_and_run(r#"<?php echo hex2bin(bin2hex("Hello"));"#);
    assert_eq!(out, "Hello");
}

// --- v0.4 batch 3: encoding, URL, base64, ctype ---

/// Verifies `htmlspecialchars()` converts `<`, `>`, `"`, `&`, and `'` to their HTML entities.
#[test]
fn test_htmlspecialchars() {
    let out = compile_and_run(r#"<?php echo htmlspecialchars("<b>\"Hi\" & 'bye'</b>");"#);
    assert_eq!(
        out,
        "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;"
    );
}

/// Verifies `htmlentities()` converts `<` and `>` to their HTML entities, encoding all applicable characters.
#[test]
fn test_htmlentities() {
    let out = compile_and_run(r#"<?php echo htmlentities("<a>");"#);
    assert_eq!(out, "&lt;a&gt;");
}

/// Verifies `html_entity_decode()` converts HTML entities back to their character equivalents.
#[test]
fn test_html_entity_decode() {
    let out = compile_and_run(r#"<?php echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;");"#);
    assert_eq!(out, "<b>hi</b>");
}

/// Verifies a roundtrip: `html_entity_decode(htmlspecialchars(...))` recovers the original string.
#[test]
fn test_htmlspecialchars_roundtrip() {
    let out = compile_and_run(
        r#"<?php echo html_entity_decode(htmlspecialchars("<div>\"test\"</div>"));"#,
    );
    assert_eq!(out, "<div>\"test\"</div>");
}

/// Verifies `urlencode()` percent-encodes spaces as `+` and special chars (`&`, `=`) as `%XX`.
#[test]
fn test_urlencode() {
    let out = compile_and_run(r#"<?php echo urlencode("hello world&foo=bar");"#);
    assert_eq!(out, "hello+world%26foo%3Dbar");
}

/// Verifies `urldecode()` decodes `+` to space and `%XX` sequences to their byte values.
#[test]
fn test_urldecode() {
    let out = compile_and_run(r#"<?php echo urldecode("hello+world%26foo%3Dbar");"#);
    assert_eq!(out, "hello world&foo=bar");
}

/// Verifies `rawurlencode()` percent-encodes all special characters including space as `%20`.
#[test]
fn test_rawurlencode() {
    let out = compile_and_run(r#"<?php echo rawurlencode("hello world");"#);
    assert_eq!(out, "hello%20world");
}

/// Verifies `rawurldecode()` decodes `%XX` sequences without touching `+` (leaves it as `+`).
#[test]
fn test_rawurldecode() {
    let out = compile_and_run(r#"<?php echo rawurldecode("hello%20world");"#);
    assert_eq!(out, "hello world");
}

/// KNOWN LIMITATION PIN — `urlencode()`/`rawurlencode()` percent-encode ASCII DIGITS, on
/// every target that generates their runtime helper assembly, not only the machine this
/// test executes on.
///
/// `src/codegen_support/runtime/strings/urlencode.rs` and `rawurlencode.rs` both check
/// A-Z, then a-z, then 0-9, using a chain of "not in this range -> check safe punctuation"
/// branches (AArch64: `cmp w12, #65; b.lt chk_safe`; x86_64: `cmp dl, 65; jb chk_safe`).
/// For ANY byte below `'A'` (0x41) — which includes every ASCII digit, 0x30-0x39 — the
/// VERY FIRST comparison already takes that early-exit branch, so the dedicated 0-9
/// comparison later in the SAME function is unreachable: a digit falls through to the
/// `- _ .` punctuation checks, misses all three, and gets percent-encoded like any other
/// unsafe byte. `urlencode("42")` answers `"%34%32"`; PHP answers `"42"`.
///
/// TRACED BY HAND THROUGH BOTH NON-AARCH64/AARCH64 CODE PATHS IN THE PINNED SOURCE, not
/// assumed: `emit_urlencode`/`emit_rawurlencode` (AArch64) and `emit_urlencode_linux_x86_64`/
/// `emit_rawurlencode_linux_x86_64` (the one x86_64 variant this project emits — there is
/// no separate macOS-x86_64 target) use the IDENTICAL check order and the IDENTICAL
/// early-exit branch for a sub-`'A'` byte, so this bug reproduces on every supported
/// target today, not only the one this test actually runs on. (An earlier review pass on
/// this branch flagged a target DIVERGENCE — AArch64 broken, x86_64 fixed. Re-reading the
/// current pinned source line by line, including simulating both branch sequences for the
/// byte `'4'` (0x34) by hand, found no such divergence: both targets share one code shape
/// and one bug. If a future change makes the targets diverge, THIS comment's claim — not
/// the earlier one — is the one to correct alongside it.)
///
/// PRE-EXISTING, NOT INTRODUCED BY curl. Found during `CURLFile`/`CURLStringFile`
/// review as a shared-builtin bug outside curl's own scope to fix — the curl
/// `CURLOPT_POSTFIELDS` array path once built a SELF-CONTAINED
/// encoder specifically to route around it for an array-to-urlencoded form that
/// has since been replaced with real `multipart/form-data`. Pinned HERE, in the
/// strings area where the bug actually lives, rather than in `tests/codegen/curl/` —
/// `crate::curl_prelude`'s multipart array walker never calls `urlencode()`/
/// `rawurlencode()` at all (binary-safe `multipart/form-data` parts need no percent
/// encoding), so this bug has no effect on curl uploads either way.
///
/// FIXED, and the expectations below are php's own, as the earlier form of this test asked for.
///
/// The safe-byte ladder tested 'A'-'Z', then 'a'-'z', then '0'-'9', and each "below this range"
/// branch jumped to the FINAL punctuation check instead of to the next range — so a digit, being
/// below 'A', was sent past its own arm on the very first comparison, and the 0-9 arm was
/// unreachable text. Testing the ranges in ASCII order costs no extra comparison and lets each
/// "below" branch fall to the next-higher range. Both arches, both encoders.
///
/// MEASURED on `php -n` 8.5.6 across fourteen shapes, so this pins more than the digits: the two
/// encoders' real difference is `~` and a space, and it is asserted here too.
#[test]
fn test_urlencode_and_rawurlencode_pass_digits_through() {
    let out = compile_and_run(
        r#"<?php
echo urlencode("42"), ":";
echo urlencode("a1b2"), ":";
echo rawurlencode("42"), ":";
echo rawurlencode("a1b2"), ":";
echo urlencode("abc9z_ ~"), ":";
echo rawurlencode("abc9z_ ~"), ":";
echo urlencode("\x00\xff9"), ":";
echo rawurlencode("a-b.c_d~e");"#,
    );
    assert_eq!(
        out,
        "42:a1b2:42:a1b2:abc9z_+%7E:abc9z_%20~:%00%FF9:a-b.c_d~e"
    );
}

/// Verifies `base64_encode()` correctly encodes "Hello" to the Base64 string "SGVsbG8=".
#[test]
fn test_base64_encode() {
    let out = compile_and_run(r#"<?php echo base64_encode("Hello");"#);
    assert_eq!(out, "SGVsbG8=");
}

/// Verifies `base64_decode()` correctly decodes the Base64 string "SGVsbG8=" to "Hello".
#[test]
fn test_base64_decode() {
    let out = compile_and_run(r#"<?php echo base64_decode("SGVsbG8=");"#);
    assert_eq!(out, "Hello");
}

/// Verifies a roundtrip: `base64_decode(base64_encode("Test 123!"))` recovers the original string.
#[test]
fn test_base64_roundtrip() {
    let out = compile_and_run(r#"<?php echo base64_decode(base64_encode("Test 123!"));"#);
    assert_eq!(out, "Test 123!");
}

/// Regression: `base64_decode()` skips embedded whitespace instead of decoding it.
///
/// The old chunked decoder consumed four raw bytes per iteration, so a space, newline, or tab
/// inside the payload shifted the rest of the input into the wrong quartet lane and produced
/// silent garbage (`"SGVs bG8="` decoded to `48656c01b1bc` instead of `Hello`). php-src's
/// reverse table marks exactly tab, LF, FF, CR, and space skippable, so all four spellings
/// below decode to `Hello`. Expected values are `LC_ALL=C php 8.4.20` output.
#[test]
fn test_base64_decode_skips_embedded_whitespace() {
    let out = compile_and_run(
        r#"<?php
echo base64_decode("SGVs bG8="), "|";
echo base64_decode("SGVs\nbG8="), "|";
echo base64_decode("SGVs\tbG8="), "|";
echo base64_decode("SGVs\r\nbG8=");
"#,
    );
    assert_eq!(out, "Hello|Hello|Hello|Hello");
}

/// Regression: `base64_decode()` decodes an unpadded final group.
///
/// The old decoder required a full four-character chunk, so it dropped the trailing group
/// entirely and returned `"Hel"` for `"SGVsbG8"`. php-src flushes whatever the accumulator
/// holds: 2 leftover characters yield 1 byte and 3 yield 2. Expected values are
/// `LC_ALL=C php 8.4.20` output.
#[test]
fn test_base64_decode_accepts_missing_padding() {
    let out = compile_and_run(
        r#"<?php
echo base64_decode("SGVsbG8"), "|";
echo bin2hex(base64_decode("ab")), "|";
echo bin2hex(base64_decode("abc")), "|";
echo bin2hex(base64_decode("a")), "|";
echo bin2hex(base64_decode("AA==")), "|";
echo bin2hex(base64_decode("AAA="));
"#,
    );
    assert_eq!(out, "Hello|69|69b7||00|0000");
}

/// Regression: a stray byte is DROPPED by the lax decoder, not folded into the output.
///
/// The old table mapped every non-alphabet byte to sextet 0, so `"SGVsbG8*"` decoded to
/// `"Hello\0"` — one byte longer than PHP's `"Hello"`. The same rule makes a `=` in the middle
/// of the payload transparent in lax mode. Expected values are `LC_ALL=C php 8.4.20` output.
#[test]
fn test_base64_decode_lax_drops_invalid_characters() {
    let out = compile_and_run(
        r#"<?php
echo bin2hex(base64_decode("SGVsbG8*")), "|";
echo bin2hex(base64_decode("SGV=sbG8=")), "|";
echo bin2hex(base64_decode("=SGVsbG8=")), "|";
echo bin2hex(base64_decode("!!!!")), "|";
echo bin2hex(base64_decode("SGVsbG8=extra"));
"#,
    );
    assert_eq!(out, "48656c6c6f|48656c6c6f|48656c6c6f||48656c6c6f1ec6dada");
}

/// Verifies the `$strict` parameter returns `false` on every input php-src rejects.
///
/// Covers all four `goto fail` paths: a byte outside the alphabet, data after a padding
/// character, a truncated one-character final group, and an invalid padding amount. Whitespace
/// stays skippable in strict mode, and an empty string is still a successful empty decode.
/// Expected values are `LC_ALL=C php 8.4.20` output.
#[test]
fn test_base64_decode_strict_mode() {
    let out = compile_and_run(
        r#"<?php
var_dump(base64_decode("SGVsbG8=", true));
var_dump(base64_decode("SGVsbG8", true));
var_dump(base64_decode("SGVs bG8=", true));
var_dump(base64_decode("SGVsbG8*", true));
var_dump(base64_decode("SGV=sbG8=", true));
var_dump(base64_decode("a", true));
var_dump(base64_decode("SGVsbG8==", true));
var_dump(base64_decode("A===", true));
var_dump(base64_decode("", true));
var_dump(base64_decode("==", true));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(5) \"Hello\"\n",
            "string(5) \"Hello\"\n",
            "string(5) \"Hello\"\n",
            "bool(false)\n",
            "bool(false)\n",
            "bool(false)\n",
            "bool(false)\n",
            "bool(false)\n",
            "string(0) \"\"\n",
            "bool(false)\n",
        )
    );
}

/// Verifies `base64_decode()` through a case-insensitive and a namespaced call site.
///
/// PHP resolves an unqualified builtin call inside a namespace by falling back to the global
/// function table, and builtin names are case-insensitive; both spellings must reach the same
/// typed runtime target as the plain lowercase call.
#[test]
fn test_base64_decode_case_insensitive_and_namespaced() {
    let out = compile_and_run(
        r#"<?php
namespace App;
echo \BASE64_DECODE("SGVsbG8="), "|";
echo Base64_Decode("SGk="), "|";
var_dump(base64_decode("SGVsbG8*", true));
"#,
    );
    assert_eq!(out, "Hello|Hi|bool(false)\n");
}

/// Verifies `base64_decode()` over an input far past the 64 KiB bounded-scratch capacity.
///
/// A 160000-character payload cannot be served from `_concat_buf`, so `__rt_concat_reserve`
/// hands back an owned heap block instead; the decode must still round-trip exactly, and the
/// strict decoder must handle a `chunk_split()`-wrapped copy whose embedded newlines are
/// skipped rather than decoded.
#[test]
fn test_base64_decode_above_scratch_capacity() {
    let out = compile_and_run(
        r#"<?php
$raw = str_repeat("elephc-base64-bounded-scratch-", 4000);
$enc = base64_encode($raw);
echo strlen($enc), "|", strlen(base64_decode($enc)), "|";
echo (base64_decode($enc) === $raw ? "roundtrip-ok" : "roundtrip-bad"), "|";
$wrapped = chunk_split($enc, 76, "\n");
echo (base64_decode($wrapped, true) === $raw ? "wrapped-ok" : "wrapped-bad"), "|";
var_dump(base64_decode($wrapped . "*", true));
"#,
    );
    assert_eq!(
        out,
        "160000|120000|roundtrip-ok|wrapped-ok|bool(false)\n"
    );
}

/// Verifies `ctype_alpha()` returns `"1"` (truthy) for an all-alphabetic string "Hello".
#[test]
fn test_gzcompress_roundtrip() {
    // gzcompress() / gzuncompress() round-trip a string through system zlib.
    let out = compile_and_run(
        r#"<?php
$data = "repeat repeat repeat repeat repeat repeat";
$packed = gzcompress($data);
echo (strlen($packed) < strlen($data) ? "smaller" : "bigger");
echo "|";
echo (gzuncompress($packed) === $data ? "roundtrip-ok" : "roundtrip-fail");
"#,
    );
    assert_eq!(out, "smaller|roundtrip-ok");
}

/// Verifies compiled PHP output for gzuncompress invalid is false.
#[test]
fn test_gzuncompress_invalid_is_false() {
    // gzuncompress() of non-zlib data returns false.
    let out = compile_and_run(
        r#"<?php echo gzuncompress("this is not zlib data") === false ? "false" : "ok";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for gzdeflate gzinflate roundtrip.
#[test]
fn test_gzdeflate_gzinflate_roundtrip() {
    // gzdeflate() / gzinflate() round-trip a string through raw DEFLATE.
    let out = compile_and_run(
        r#"<?php
$data = str_repeat("raw deflate raw deflate ", 16);
$packed = gzdeflate($data);
echo (strlen($packed) < strlen($data) ? "smaller" : "bigger");
echo "|";
echo (gzinflate($packed) === $data ? "roundtrip-ok" : "roundtrip-fail");
"#,
    );
    assert_eq!(out, "smaller|roundtrip-ok");
}

/// Verifies compiled PHP output for gzinflate invalid is false.
#[test]
fn test_gzinflate_invalid_is_false() {
    // gzinflate() of data that is not raw DEFLATE returns false.
    let out = compile_and_run(
        r#"<?php echo gzinflate("this is not deflate data") === false ? "false" : "ok";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for gzinflate decodes zlib deflate filter.
#[test]
fn test_gzinflate_decodes_zlib_deflate_filter() {
    // gzinflate() decodes the raw DEFLATE produced by the zlib.deflate stream
    // filter — the two zlib features agree on the wire format.
    let out = compile_and_run(
        r#"<?php
$data = str_repeat("filter and builtin agree. ", 20);
$w = fopen("filtered.bin", "w");
stream_filter_append($w, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($w, $data);
fclose($w);
echo (gzinflate(file_get_contents("filtered.bin")) === $data ? "decoded-ok" : "FAIL");
"#,
    );
    assert_eq!(out, "decoded-ok");
}

/// Verifies compiled PHP output for gz builtins case insensitive.
#[test]
fn test_gz_builtins_case_insensitive() {
    // PHP builtin names are case-insensitive.
    let out = compile_and_run(
        r#"<?php $s = "case test case test"; echo GZINFLATE(GzDeflate($s)) === $s ? "ci-ok" : "FAIL";"#,
    );
    assert_eq!(out, "ci-ok");
}

/// Verifies compiled PHP output for ctype alpha true.
#[test]
fn test_ctype_alpha_true() {
    let out = compile_and_run(r#"<?php echo ctype_alpha("Hello");"#);
    assert_eq!(out, "1");
}

/// Verifies `ctype_alpha()` returns `""` (empty/falsy) for a string containing digits "Hello123".
#[test]
fn test_ctype_alpha_false() {
    let out = compile_and_run(r#"<?php echo ctype_alpha("Hello123");"#);
    assert_eq!(out, "");
}

/// Verifies `ctype_digit()` returns `"1"` (truthy) for an all-digit string "12345".
#[test]
fn test_ctype_digit_true() {
    let out = compile_and_run(r#"<?php echo ctype_digit("12345");"#);
    assert_eq!(out, "1");
}

/// Verifies `ctype_digit()` returns `""` (empty/falsy) for a string containing letters "123abc".
#[test]
fn test_ctype_digit_false() {
    let out = compile_and_run(r#"<?php echo ctype_digit("123abc");"#);
    assert_eq!(out, "");
}

/// Verifies `ctype_alnum()` returns `"1"` (truthy) for an alphanumeric string "Hello123".
#[test]
fn test_ctype_alnum_true() {
    let out = compile_and_run(r#"<?php echo ctype_alnum("Hello123");"#);
    assert_eq!(out, "1");
}

/// Verifies `ctype_alnum()` returns `""` (empty/falsy) for a string containing a space "Hello 123".
#[test]
fn test_ctype_alnum_false() {
    let out = compile_and_run(r#"<?php echo ctype_alnum("Hello 123");"#);
    assert_eq!(out, "");
}

/// Verifies `ctype_space()` returns `"1"` (truthy) for a string containing only whitespace characters.
#[test]
fn test_ctype_space_true() {
    let out = compile_and_run("<?php echo ctype_space(\" \\t\\n\");");
    assert_eq!(out, "1");
}

/// Verifies `ctype_space()` returns `""` (empty/falsy) for a non-whitespace alphabetic string.
#[test]
fn test_ctype_space_false() {
    let out = compile_and_run(r#"<?php echo ctype_space("hello");"#);
    assert_eq!(out, "");
}

// --- sprintf / printf ---

/// Verifies `sprintf()` with `%x` format produces lowercase hex output for decimal 255.
#[test]
fn test_sprintf_hex() {
    let out = compile_and_run(r#"<?php echo sprintf("%x", 255);"#);
    assert_eq!(out, "ff");
}

// --- long2ip ---

/// Verifies compiled PHP output for long2ip private address.
#[test]
fn test_long2ip_private_address() {
    let out = compile_and_run(r#"<?php echo long2ip(3232235777);"#);
    assert_eq!(out, "192.168.1.1");
}

/// Verifies compiled PHP output for long2ip loopback.
#[test]
fn test_long2ip_loopback() {
    let out = compile_and_run(r#"<?php echo long2ip(2130706433);"#);
    assert_eq!(out, "127.0.0.1");
}

/// Verifies compiled PHP output for long2ip zero and broadcast.
#[test]
fn test_long2ip_zero_and_broadcast() {
    let out = compile_and_run(r#"<?php echo long2ip(0) . "|" . long2ip(4294967295);"#);
    assert_eq!(out, "0.0.0.0|255.255.255.255");
}

// --- ip2long ---

/// Verifies compiled PHP output for ip2long valid addresses.
#[test]
fn test_ip2long_valid_addresses() {
    let out = compile_and_run(
        r#"<?php echo ip2long("192.168.1.1") . "|" . ip2long("0.0.0.0") . "|" . ip2long("255.255.255.255");"#,
    );
    assert_eq!(out, "3232235777|0|4294967295");
}

/// Verifies compiled PHP output for ip2long rejects invalid.
#[test]
fn test_ip2long_rejects_invalid() {
    let out = compile_and_run(
        r#"<?php
echo ip2long("not.an.ip") === false ? "a" : "A";
echo ip2long("1.2.3") === false ? "b" : "B";
echo ip2long("256.0.0.1") === false ? "c" : "C";
echo ip2long("1.2.3.4.5") === false ? "d" : "D";
"#,
    );
    assert_eq!(out, "abcd");
}

// --- inet_ntop / inet_pton ---

/// Verifies compiled PHP output for inet ntop ipv4.
#[test]
fn test_inet_ntop_ipv4() {
    let out = compile_and_run(r#"<?php echo inet_ntop(chr(192) . chr(168) . chr(0) . chr(1));"#);
    assert_eq!(out, "192.168.0.1");
}

/// Verifies compiled PHP output for inet ntop loopback.
#[test]
fn test_inet_ntop_loopback() {
    let out = compile_and_run(r#"<?php echo inet_ntop(chr(127) . chr(0) . chr(0) . chr(1));"#);
    assert_eq!(out, "127.0.0.1");
}

/// Verifies compiled PHP output for inet ntop rejects wrong length.
#[test]
fn test_inet_ntop_rejects_wrong_length() {
    let out = compile_and_run(r#"<?php var_dump(inet_ntop("xx"));"#);
    assert_eq!(out, "bool(false)\n");
}

/// Verifies compiled PHP output for inet pton valid and invalid.
#[test]
fn test_inet_pton_valid_and_invalid() {
    let out = compile_and_run(
        r#"<?php
echo inet_pton("1.2.3.4") === false ? "F" : "S";
echo inet_pton("nonsense") === false ? "F" : "S";
"#,
    );
    assert_eq!(out, "SF");
}

/// EC-11 (#506): htmlspecialchars()/htmlentities() accept the optional ENT_* flags argument
/// (the common `htmlspecialchars($s, ENT_QUOTES)` form) and the ENT_* constants resolve to their
/// PHP values. Byte-parity vs PHP 8.5 for ENT_QUOTES escaping (the runtime applies ENT_QUOTES).
#[test]
fn test_htmlspecialchars_ent_flags() {
    assert_eq!(
        compile_and_run("<?php echo htmlspecialchars('<b> & x', ENT_QUOTES), '|', htmlentities('<y>', ENT_QUOTES), '|', (ENT_QUOTES + ENT_HTML5);"),
        "&lt;b&gt; &amp; x|&lt;y&gt;|51"
    );
}

/// Regression: the shared `lower_html_escape` emitter must name the builtin that was actually
/// called in its argument-coercion diagnostic. `htmlentities()` with an uncoercible (array)
/// subject previously reported "htmlspecialchars string coercion ..." instead of htmlentities.
#[test]
fn test_htmlentities_coercion_error_names_htmlentities() {
    let dir = make_cli_test_dir("elephc_htmlentities_diag");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo htmlentities([1, 2]);").unwrap();

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "expected the compile to fail on an array subject, got success; stderr={stderr}"
    );
    assert!(
        stderr.contains("htmlentities string coercion"),
        "coercion diagnostic must name htmlentities, got stderr={stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `bin2hex()` of a 100 KB payload — whose 200 KB hexadecimal expansion cannot fit the
/// shared 64 KiB concat scratch buffer — produces the full correct result instead of writing past
/// the scratch end into the adjacent BSS globals (concat-scratch overflow regression).
#[test]
fn test_bin2hex_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("\x00\x11\xfe", 40000);
$h = bin2hex($s);
echo strlen($h), "|", substr($h, 0, 6), "|", substr($h, -6);
"#,
    );
    assert_eq!(out, "240000|0011fe|0011fe");
}

/// Verifies `base64_encode()` / `base64_decode()` round-trip a payload whose encoding exceeds the
/// 64 KiB concat scratch buffer, so both directions take the heap fallback and stay byte-exact.
#[test]
fn test_base64_roundtrip_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("elephc", 20000);
$e = base64_encode($s);
$d = base64_decode($e);
echo strlen($e), "|", substr($e, 0, 8), "|", strlen($d), "|", ($d === $s ? "same" : "DIFF");
"#,
    );
    assert_eq!(out, "160000|ZWxlcGhj|120000|same");
}

/// Verifies `urlencode()` of a payload whose worst-case 3x percent-encoded expansion exceeds the
/// 64 KiB concat scratch buffer keeps every escape intact through the heap fallback.
#[test]
fn test_urlencode_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = str_repeat("%~", 30000);
$e = urlencode($s);
echo strlen($e), "|", substr($e, 0, 6), "|", substr($e, -6);
"#,
    );
    assert_eq!(out, "180000|%25%7E|%25%7E");
}

/// Verifies `hex2bin()` decoding a hexadecimal string longer than the 64 KiB concat scratch
/// buffer still produces the exact binary payload.
#[test]
fn test_hex2bin_input_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$h = str_repeat("41", 70000);
$b = hex2bin($h);
echo strlen($b), "|", substr($b, 0, 3), "|", substr($b, -3);
"#,
    );
    assert_eq!(out, "70000|AAA|AAA");
}

/// Verifies `rawurlencode()` of a payload whose worst-case 3x percent-encoded expansion exceeds
/// the 64 KiB concat scratch buffer keeps every RFC 3986 escape intact through the heap fallback.
#[test]
fn test_rawurlencode_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$e = rawurlencode(str_repeat("%~ ", 30000));
echo strlen($e), "|", substr($e, 0, 9), "|", substr($e, -9);
"#,
    );
    assert_eq!(out, "210000|%25~%20%2|20%25~%20");
}

/// Verifies `urldecode()` of a percent-encoded payload longer than the 64 KiB concat scratch
/// buffer decodes every escape through the heap fallback.
#[test]
fn test_urldecode_input_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$u = urldecode(str_repeat("%41+", 30000));
echo strlen($u), "|", substr($u, 0, 4), "|", substr($u, -4);
"#,
    );
    assert_eq!(out, "60000|A A |A A ");
}

/// Verifies `quotemeta()` escapes every php-src metacharacter and leaves other bytes alone.
#[test]
fn test_quotemeta() {
    let out = compile_and_run(
        r#"<?php echo quotemeta("Hello world. (can you hear me?) [yes] \$5 + 3 * 2 = 11 \\ ^end");"#,
    );
    assert_eq!(
        out,
        r#"Hello world\. \(can you hear me\?\) \[yes\] \$5 \+ 3 \* 2 = 11 \\ \^end"#
    );
}

/// Verifies `quotemeta()` returns an empty string unchanged and passes non-metacharacters through.
#[test]
fn test_quotemeta_empty_and_plain() {
    let out = compile_and_run(
        r#"<?php echo "|", quotemeta(""), "|", quotemeta("no specials here"), "|";"#,
    );
    assert_eq!(out, "||no specials here|");
}

/// Verifies `quotemeta()` resolves through case-insensitive and namespaced call forms.
#[test]
fn test_quotemeta_case_insensitive_and_namespaced() {
    let out = compile_and_run(r#"<?php echo QuoteMeta("A.B"), "|", \quotemeta("C*D");"#);
    assert_eq!(out, "A\\.B|C\\*D");
}

/// Verifies `quotemeta()` of a payload whose worst-case 2x expansion exceeds the 64 KiB concat
/// scratch buffer keeps every escape intact through the bounded heap fallback.
#[test]
fn test_quotemeta_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$q = quotemeta(str_repeat("a.b(c)", 20000));
echo strlen($q), "|", substr($q, 0, 10), "|", substr($q, -10);
"#,
    );
    assert_eq!(out, "180000|a\\.b\\(c\\)a|)a\\.b\\(c\\)");
}

/// Verifies `chunk_split()` appends the separator after every chunk, including the trailing
/// partial one, and reproduces php-src's lone-separator result for an empty subject.
#[test]
fn test_chunk_split() {
    let out = compile_and_run(
        r#"<?php
echo chunk_split("abcdefgh", 3, "-"), "|";
echo chunk_split("abcdef", 3, "-"), "|";
echo chunk_split("", 3, "-"), "|";
echo chunk_split("ab", 5, "|");
"#,
    );
    assert_eq!(out, "abc-def-gh-|abc-def-|-|ab|");
}

/// Verifies `chunk_split()` defaults to 76-byte chunks joined by CRLF and accepts an empty
/// separator without inserting anything.
#[test]
fn test_chunk_split_defaults_and_empty_separator() {
    let out = compile_and_run(
        r#"<?php
echo bin2hex(chunk_split("abc")), "|", chunk_split("abcdefgh", 3, "");
"#,
    );
    assert_eq!(out, "6162630d0a|abcdefgh");
}

/// Verifies `chunk_split()` resolves through case-insensitive and namespaced call forms.
#[test]
fn test_chunk_split_case_insensitive_and_namespaced() {
    let out = compile_and_run(
        r#"<?php echo Chunk_Split("xyz", 1, "."), "|", \chunk_split("xyz", 2, ".");"#,
    );
    assert_eq!(out, "x.y.z.|xy.z.");
}

/// Verifies `chunk_split()` raises php-src's `ValueError` for a non-positive `$length`.
#[test]
fn test_chunk_split_non_positive_length_throws() {
    let out = compile_and_run(
        r#"<?php
try { chunk_split("ab", 0, "|"); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { chunk_split("ab", -1, "|"); } catch (\ValueError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "chunk_split(): Argument #2 ($length) must be greater than 0\nchunk_split(): Argument #2 ($length) must be greater than 0"
    );
}

/// Verifies `chunk_split()` of a result larger than the 64 KiB concat scratch buffer keeps
/// every chunk boundary intact through the bounded heap fallback.
#[test]
fn test_chunk_split_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$s = chunk_split(str_repeat("A", 80000), 40, "==");
echo strlen($s), "|", substr($s, 38, 6), "|", substr($s, -5);
"#,
    );
    assert_eq!(out, "84000|AA==AA|AAA==");
}

/// Verifies `str_word_count()` counts php-src's words and returns them as a plain list,
/// including the apostrophe-joined form and the empty-subject shortcuts.
#[test]
fn test_str_word_count() {
    let out = compile_and_run(
        r#"<?php
echo str_word_count("Hello friend, you're looking          good today!"), "|";
echo implode(",", str_word_count("Hello friend, you're looking good today!", 1)), "|";
echo str_word_count(""), "|", count(str_word_count("", 1));
"#,
    );
    assert_eq!(out, "6|Hello,friend,you're,looking,good,today|0|0");
}

/// Verifies `str_word_count()` format 2 keys every word by its byte offset in the subject.
#[test]
fn test_str_word_count_offset_map() {
    let out = compile_and_run(
        r#"<?php
foreach (str_word_count("Hello friend, you're here", 2) as $offset => $word) { echo $offset, ":", $word, " "; }
"#,
    );
    assert_eq!(out, "0:Hello 6:friend 14:you're 21:here ");
}

/// Verifies `str_word_count()` honours the extra `$characters` alphabet and php-src's rule
/// that a leading `'`/`-` and a trailing `-` are dropped unless the list re-admits them.
#[test]
fn test_str_word_count_characters_and_boundaries() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", str_word_count("fri3nd", 1)), "|";
echo implode(",", str_word_count("fri3nd", 1, "3")), "|";
echo implode(",", str_word_count("-abc-", 1)), "|";
echo implode(",", str_word_count("-abc-", 1, "-")), "|";
echo implode(",", str_word_count("'abc'", 1)), "|";
echo implode(",", str_word_count("a-b'c", 1));
"#,
    );
    assert_eq!(out, "fri,nd|fri3nd|abc|-abc-|abc'|a-b'c");
}

/// Verifies `str_word_count()` resolves through case-insensitive, namespaced, and named
/// argument call forms.
#[test]
fn test_str_word_count_case_insensitive_and_named() {
    let out = compile_and_run(
        r#"<?php echo Str_Word_Count("a b c"), "|", \str_word_count("a b c"), "|", str_word_count(string: "a b", format: 1)[1];"#,
    );
    assert_eq!(out, "3|3|b");
}

/// Verifies `str_word_count()` raises php-src's `ValueError` for a `$format` outside 0..2.
#[test]
fn test_str_word_count_invalid_format_throws() {
    let out = compile_and_run(
        r#"<?php
try { str_word_count("ab", 3); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { str_word_count("ab", -1); } catch (\ValueError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "str_word_count(): Argument #2 ($format) must be a valid format value\nstr_word_count(): Argument #2 ($format) must be a valid format value"
    );
}

/// Verifies `str_word_count()` format 1 keeps growing its result array well past the initial
/// capacity, so the appended words survive every reallocation.
#[test]
fn test_str_word_count_list_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$words = str_word_count(str_repeat("alpha beta ", 8000), 1);
echo count($words), "|", $words[0], "|", $words[15999];
"#,
    );
    assert_eq!(out, "16000|alpha|beta");
}

/// Verifies `count_chars()` returns the used-byte tally for mode 1 and the used / unused byte
/// lists for modes 3 and 4.
#[test]
fn test_count_chars_modes() {
    let out = compile_and_run(
        r#"<?php
$used = count_chars("hello world", 1);
foreach ($used as $byte => $count) { echo $byte, "=", $count, " "; }
echo "|", count_chars("hello world", 3), "|", strlen(count_chars("hello world", 4));
"#,
    );
    assert_eq!(
        out,
        "32=1 100=1 101=1 104=1 108=3 111=2 114=1 119=1 | dehlorw|248"
    );
}

/// Verifies `count_chars()` mode 0 (and the omitted default) tallies all 256 byte values while
/// mode 2 keeps only the ones the subject never uses.
#[test]
fn test_count_chars_full_and_unused_tallies() {
    let out = compile_and_run(
        r#"<?php
$all = count_chars("aab", 0);
$unused = count_chars("aab", 2);
$default = count_chars("aab");
echo count($all), "|", $all[97], "|", $all[98], "|", $all[0], "|";
echo count($unused), "|", $unused[0], "|", (isset($unused[97]) ? "y" : "n"), "|";
echo count($default), "|", $default[97];
"#,
    );
    assert_eq!(out, "256|2|1|0|254|0|n|256|2");
}

/// Verifies `count_chars()` returns php-src's empty results for an empty subject.
#[test]
fn test_count_chars_empty_subject() {
    let out = compile_and_run(
        r#"<?php
echo count(count_chars("", 1)), "|", strlen(count_chars("", 3)), "|", strlen(count_chars("", 4)), "|", count(count_chars("", 2));
"#,
    );
    assert_eq!(out, "0|0|256|256");
}

/// Verifies `count_chars()` resolves through case-insensitive, namespaced, and named argument
/// call forms.
#[test]
fn test_count_chars_case_insensitive_and_named() {
    let out = compile_and_run(
        r#"<?php echo Count_Chars("abc", 3), "|", \count_chars("cba", 3), "|", count_chars(string: "zya", mode: 3), "|", count(count_chars(string: "aab", mode: 1));"#,
    );
    assert_eq!(out, "abc|abc|ayz|2");
}

/// Verifies `count_chars()` raises php-src's `ValueError` for a `$mode` outside 0..4.
#[test]
fn test_count_chars_invalid_mode_throws() {
    let out = compile_and_run(
        r#"<?php
try { count_chars("ab", 5); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { count_chars("ab", -1); } catch (\ValueError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "count_chars(): Argument #2 ($mode) must be between 0 and 4 (inclusive)\ncount_chars(): Argument #2 ($mode) must be between 0 and 4 (inclusive)"
    );
}

/// Verifies `strtr()` replacement pairs apply longest-match-first in one left-to-right pass
/// with no re-substitution of already replaced text.
#[test]
fn test_strtr_replacement_pairs() {
    let out = compile_and_run(
        r#"<?php
echo strtr("foo bar", ["foo"=>"bar","bar"=>"baz"]), "|";
echo strtr("hi all, I said hello", ["hello"=>"hi","hi"=>"hello"]), "|";
echo strtr("abc", ["a"=>"b","ab"=>"X"]), "|";
echo strtr("abcabc", ["abc"=>"x","bca"=>"y"]), "|";
echo strtr("aXbXc", ["X"=>"","b"=>"BB"]);
"#,
    );
    assert_eq!(out, "bar baz|hello all, I said hi|Xc|xx|aBBc");
}

/// Verifies `strtr()` skips keys longer than the subject, matches numeric-string and integer
/// keys through their decimal spelling, and returns the subject for an empty pair list.
#[test]
fn test_strtr_key_edge_cases() {
    let out = compile_and_run(
        r#"<?php
echo strtr("abc", ["abcd"=>"X"]), "|";
echo strtr("12345", [1=>"one", 23=>"two-three"]), "|";
echo strtr("0a1", ["0"=>"zero","1"=>"one"]), "|";
echo strtr("abc", []), "|";
echo strtr("abc", ["a","b"]);
"#,
    );
    assert_eq!(out, "abc|onetwo-three45|zeroaone|abc|abc");
}

/// Verifies the three-argument `strtr()` byte translation truncates to the shorter list, never
/// re-translates an already written byte, and lets a later pair win for the same source byte.
#[test]
fn test_strtr_pairwise() {
    let out = compile_and_run(
        r#"<?php
echo strtr("abcd", "abc", "xy"), "|";
echo strtr("abcd", "ab", "xyz"), "|";
echo strtr("abcd", "", ""), "|";
echo strtr("aab", "ab", "ba"), "|";
echo strtr("a", "aa", "xy");
"#,
    );
    assert_eq!(out, "xycd|xycd|abcd|bba|y");
}

/// Verifies `strtr()` resolves through case-insensitive, namespaced, and named argument call
/// forms in both of its shapes.
#[test]
fn test_strtr_case_insensitive_and_named() {
    let out = compile_and_run(
        r#"<?php $map = ["b"=>"B"]; echo StrTr("abc", "abc", "xyz"), "|", \strtr("abc", ["a"=>"1"]), "|", strtr(string: "abc", from: $map), "|", strtr(string: "abc", from: "ab", to: "xy");"#,
    );
    assert_eq!(out, "xyz|1bc|aBc|xyc");
}

/// Verifies a `strtr()` result far larger than the 64 KiB concat scratch buffer stays intact
/// through the bounded heap fallback.
#[test]
fn test_strtr_result_larger_than_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
$out = strtr(str_repeat("ab", 50000), ["ab" => "cdef"]);
echo strlen($out), "|", substr($out, 0, 8), "|", substr($out, -8);
"#,
    );
    assert_eq!(out, "200000|cdefcdef|cdefcdef");
}

/// Verifies `quoted_printable_encode()` escapes exactly the byte classes php-src escapes.
///
/// Control bytes, `0x7F`, high-bit bytes, and `=` itself always become `=XX`; ordinary
/// printable ASCII is copied through. A TRAILING space stays a literal space (php-src only
/// escapes a space that is directly followed by a `CR`), while a trailing tab is a control
/// byte and always becomes `=09`. Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_quoted_printable_encode_escapes_php_byte_classes() {
    let out = compile_and_run(
        r#"<?php
echo quoted_printable_encode("Hello, World!"), "|";
echo quoted_printable_encode(""), "|";
echo quoted_printable_encode("a=b=c"), "|";
echo quoted_printable_encode("caf\xC3\xA9"), "|";
echo quoted_printable_encode("\x00\x01\x1F\x7F\x80"), "|";
echo quoted_printable_encode("a\tb"), "|";
echo quoted_printable_encode("a "), "|";
echo quoted_printable_encode("a\t");
"#,
    );
    assert_eq!(
        out,
        "Hello, World!||a=3Db=3Dc|caf=C3=A9|=00=01=1F=7F=80|a=09b|a |a=09"
    );
}

/// Verifies `quoted_printable_encode()` line-ending handling.
///
/// An embedded `CRLF` is a hard line break and is copied through unchanged, but a lone `CR`
/// or `LF` is an ordinary control byte and becomes `=0D`/`=0A`. A space directly before a
/// `CRLF` is escaped so transport cannot strip it. Expected values are verbatim
/// `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_quoted_printable_encode_line_endings() {
    let out = compile_and_run(
        r#"<?php
echo bin2hex(quoted_printable_encode("a \r\nb")), "|";
echo bin2hex(quoted_printable_encode("a\r\nb")), "|";
echo bin2hex(quoted_printable_encode("a\nb")), "|";
echo bin2hex(quoted_printable_encode("a\rb")), "|";
echo bin2hex(quoted_printable_encode("line1\r\nline2\r\n"));
"#,
    );
    assert_eq!(
        out,
        "613d32300d0a62|610d0a62|613d304162|613d304462|6c696e65310d0a6c696e65320d0a"
    );
}

/// Verifies the 76-character soft line break: 75 columns of payload followed by a trailing `=`
/// and a `CRLF`.
///
/// A 75-byte line is emitted whole; the 76th byte moves to a new line behind `=\r\n`
/// (`...61 3d 0d 0a 61`). 30 `=` characters encode to 93 bytes — more than php-src's own
/// `3 * length` allocation bound, which is why the runtime reserves `4 * len + 8`. The last
/// two cases pin php-src's UTF-8 lookahead allowance: a 2-byte lead breaks one column earlier
/// than an ASCII escape and a 3-byte lead two columns earlier, so a character is never split
/// across the fold. Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_quoted_printable_encode_soft_line_breaks() {
    let out = compile_and_run(
        r#"<?php
echo strlen(quoted_printable_encode(str_repeat("a", 75))), "|";
echo bin2hex(quoted_printable_encode(str_repeat("a", 76))), "|";
echo strlen(quoted_printable_encode(str_repeat("=", 30))), "|";
echo bin2hex(quoted_printable_encode(str_repeat("a", 74) . "\xC3\xA9")), "|";
echo bin2hex(quoted_printable_encode(str_repeat("a", 73) . "\xE2\x82\xAC"));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "75|6161616161616161616161616161616161616161616161616161616161616161616161616",
            "1616161616161616161616161616161616161616161616161616161616161616161616161616",
            "13d0d0a61|93|616161616161616161616161616161616161616161616161616161616161616",
            "1616161616161616161616161616161616161616161616161616161616161616161616161616",
            "1616161613d0d0a3d43333d4139|616161616161616161616161616161616161616161616161",
            "6161616161616161616161616161616161616161616161616161616161616161616161616161",
            "61616161616161616161613d0d0a3d45323d38323d4143",
        )
    );
}

/// Verifies `quoted_printable_encode()` through case-insensitive, namespaced, named-argument,
/// and dynamic call sites, so the registry catalog resolves every spelling to one target.
#[test]
fn test_quoted_printable_encode_case_insensitive_and_namespaced() {
    let out = compile_and_run(
        r#"<?php
namespace App;
echo \QUOTED_PRINTABLE_ENCODE("caf\xC3\xA9"), "|";
echo Quoted_Printable_Encode("a=b"), "|";
echo quoted_printable_encode(string: "x\tz"), "|";
echo call_user_func('quoted_printable_encode', "= ");
"#,
    );
    assert_eq!(out, "caf=C3=A9|a=3Db|x=09z|=3D ");
}

/// Verifies `quoted_printable_encode()` over a result far past the 64 KiB bounded-scratch
/// capacity, so `__rt_concat_reserve` serves the reservation from an owned heap block.
/// Expected values are verbatim `LC_ALL=C php` 8.4.20 output.
#[test]
fn test_quoted_printable_encode_above_scratch_capacity() {
    let out = compile_and_run(
        r#"<?php
$raw = str_repeat("=\xC3\xA9 x", 20000);
$out = quoted_printable_encode($raw);
echo strlen($raw), "|", strlen($out), "|", md5($out);
"#,
    );
    assert_eq!(out, "100000|228997|e5e2d387e026fd978522763ba791144f");
}
