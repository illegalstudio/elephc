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

/// Verifies `wordwrap()` breaks a string at the specified column width (15) with "\n" delimiter.
#[test]
fn test_wordwrap() {
    let out = compile_and_run(
        r#"<?php echo wordwrap("The quick brown fox jumped over the lazy dog", 15, "\n");"#,
    );
    assert!(out.contains('\n'));
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

#[test]
fn test_gzuncompress_invalid_is_false() {
    // gzuncompress() of non-zlib data returns false.
    let out = compile_and_run(
        r#"<?php echo gzuncompress("this is not zlib data") === false ? "false" : "ok";"#,
    );
    assert_eq!(out, "false");
}

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

#[test]
fn test_gzinflate_invalid_is_false() {
    // gzinflate() of data that is not raw DEFLATE returns false.
    let out = compile_and_run(
        r#"<?php echo gzinflate("this is not deflate data") === false ? "false" : "ok";"#,
    );
    assert_eq!(out, "false");
}

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

#[test]
fn test_gz_builtins_case_insensitive() {
    // PHP builtin names are case-insensitive.
    let out = compile_and_run(
        r#"<?php $s = "case test case test"; echo GZINFLATE(GzDeflate($s)) === $s ? "ci-ok" : "FAIL";"#,
    );
    assert_eq!(out, "ci-ok");
}

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

#[test]
fn test_long2ip_private_address() {
    let out = compile_and_run(r#"<?php echo long2ip(3232235777);"#);
    assert_eq!(out, "192.168.1.1");
}

#[test]
fn test_long2ip_loopback() {
    let out = compile_and_run(r#"<?php echo long2ip(2130706433);"#);
    assert_eq!(out, "127.0.0.1");
}

#[test]
fn test_long2ip_zero_and_broadcast() {
    let out = compile_and_run(r#"<?php echo long2ip(0) . "|" . long2ip(4294967295);"#);
    assert_eq!(out, "0.0.0.0|255.255.255.255");
}

// --- ip2long ---

#[test]
fn test_ip2long_valid_addresses() {
    let out = compile_and_run(
        r#"<?php echo ip2long("192.168.1.1") . "|" . ip2long("0.0.0.0") . "|" . ip2long("255.255.255.255");"#,
    );
    assert_eq!(out, "3232235777|0|4294967295");
}

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

#[test]
fn test_inet_ntop_ipv4() {
    let out = compile_and_run(r#"<?php echo inet_ntop(chr(192) . chr(168) . chr(0) . chr(1));"#);
    assert_eq!(out, "192.168.0.1");
}

#[test]
fn test_inet_ntop_loopback() {
    let out = compile_and_run(r#"<?php echo inet_ntop(chr(127) . chr(0) . chr(0) . chr(1));"#);
    assert_eq!(out, "127.0.0.1");
}

#[test]
fn test_inet_ntop_rejects_wrong_length() {
    let out = compile_and_run(r#"<?php var_dump(inet_ntop("xx"));"#);
    assert_eq!(out, "bool(false)\n");
}

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
