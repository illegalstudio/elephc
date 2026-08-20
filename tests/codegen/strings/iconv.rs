//! Purpose:
//! End-to-end tests for PHP's iconv extension on the native compilation path.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected values were captured from PHP 8.3 CLI, including the diagnostic wording.
//! - Only charsets every supported platform's iconv provides are used (`UTF-8`,
//!   `ISO-8859-1`, `ASCII`, `UCS-4LE`), so the fixtures stay target-independent.
//! - Byte-exact results are asserted through `bin2hex()` wherever a charset other than
//!   UTF-8 is involved, so the assertions do not depend on the test harness's encoding.

use crate::support::*;

/// Verifies `iconv()` transcodes in both directions and preserves representable bytes.
#[test]
fn test_iconv_round_trips_between_charsets() {
    let out = compile_and_run(
        "<?php $latin1 = iconv('UTF-8', 'ISO-8859-1', 'café');
echo bin2hex($latin1), ':', iconv('ISO-8859-1', 'UTF-8', $latin1);",
    );
    assert_eq!(out, "636166e9:café");
}

/// Verifies `iconv()` honors libc's `//IGNORE` and `//TRANSLIT` target suffixes.
///
/// `//TRANSLIT`'s approximation comes from the platform's iconv, exactly as it does for
/// PHP: glibc renders `é` as `e`, GNU libiconv as `'e`. `//IGNORE` drops the character
/// outright, so it reads the same everywhere.
#[test]
fn test_iconv_supports_translit_and_ignore_suffixes() {
    let out = compile_and_run(
        "<?php echo iconv('UTF-8', 'ASCII//TRANSLIT', 'héllo'), ':',
iconv('UTF-8', 'ISO-8859-1//IGNORE', 'a日本b');",
    );
    let expected = if cfg!(target_os = "macos") { "h'ello:ab" } else { "hello:ab" };
    assert_eq!(out, expected);
}

/// Verifies a case-insensitive and a namespaced call reach the same builtin.
#[test]
fn test_iconv_accepts_case_insensitive_and_namespaced_calls() {
    let out = compile_and_run(
        "<?php echo ICONV_STRLEN('héllo'), ':', \\iconv_strlen('héllo');",
    );
    assert_eq!(out, "5:5");
}

/// Verifies an unusable charset pair warns and answers PHP `false`.
#[test]
fn test_iconv_reports_an_unknown_charset() {
    let out = compile_and_run_capture(
        "<?php var_dump(iconv('NOPEENC', 'UTF-8', 'x'));",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
    assert!(
        out.stderr.contains(
            "Warning: iconv(): Wrong encoding, conversion from \"NOPEENC\" to \"UTF-8\" is not allowed"
        ),
        "missing charset warning: {}",
        out.stderr
    );
}

/// Verifies a malformed byte sequence produces php-src's notice and PHP `false`.
#[test]
fn test_iconv_reports_malformed_input() {
    let out = compile_and_run_capture(
        "<?php var_dump(iconv('UTF-8', 'UTF-8', \"abc\\xC3(def\"));",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
    assert!(
        out.stderr
            .contains("Notice: iconv(): Detected an illegal character in input string"),
        "missing malformed-input notice: {}",
        out.stderr
    );
}

/// Verifies `iconv_strlen()` counts characters, not bytes, in the selected charset.
#[test]
fn test_iconv_strlen_counts_characters() {
    let out = compile_and_run(
        "<?php echo iconv_strlen('héllo'), ':', iconv_strlen('héllo', 'ISO-8859-1'), ':',
iconv_strlen(''), ':', iconv_strlen('héllo', null);",
    );
    assert_eq!(out, "5:6:0:5");
}

/// Verifies `iconv_substr()` follows PHP's negative-offset and negative-length rules.
#[test]
fn test_iconv_substr_slices_by_characters() {
    let out = compile_and_run(
        "<?php echo iconv_substr('héllo', 1, 3), ':', iconv_substr('héllo', -3), ':',
iconv_substr('héllo', 1, -1), ':', iconv_substr('héllo', 10), ':',
iconv_substr('héllo', 1, 0), ':', iconv_substr('héllo', 1);",
    );
    assert_eq!(out, "éll:llo:éll:::éllo");
}

/// Verifies both search builtins report character positions and PHP's miss result.
#[test]
fn test_iconv_search_reports_character_positions() {
    let out = compile_and_run(
        "<?php echo iconv_strpos('héllo', 'l'), ':', iconv_strpos('héllo', 'l', 3), ':',
iconv_strpos('héllo', 'l', -2), ':', iconv_strrpos('abcabc', 'bc'), ':',
var_export(iconv_strpos('héllo', 'z'), true), ':',
var_export(iconv_strpos('héllo', ''), true);",
    );
    assert_eq!(out, "2:3:3:4:false:false");
}

/// Verifies an `$offset` outside the haystack raises PHP 8's catchable `ValueError`.
#[test]
fn test_iconv_strpos_offset_out_of_range_throws() {
    let out = compile_and_run(
        "<?php try { iconv_strpos('héllo', 'l', 99); } catch (\\ValueError $e) {
echo get_class($e), '|', $e->getMessage();
}",
    );
    assert_eq!(
        out,
        "ValueError|iconv_strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    );
}

/// Verifies `iconv_mime_encode()` folds base64 encoded-words at the default line length.
#[test]
fn test_iconv_mime_encode_folds_base64_words() {
    let out = compile_and_run(
        "<?php echo str_replace(\"\\r\\n\", '|', iconv_mime_encode('Subject',
'Prüfung Prüfung Prüfung Prüfung Prüfung Prüfung'));",
    );
    assert_eq!(
        out,
        "Subject: =?UTF-8?B?UHLDvGZ1bmcgUHLDvGZ1bmcgUHLDvGZ1bmcgUHLDvGZ1bmc=?=| \
         =?UTF-8?B?IFByw7xmdW5nIFByw7xmdW5n?="
    );
}

/// Verifies `iconv_mime_encode()` reads its `$options` array at the call site.
#[test]
fn test_iconv_mime_encode_reads_options() {
    let out = compile_and_run(
        "<?php echo iconv_mime_encode('Subject', 'Prüfung', ['scheme' => 'Q']), '|',
iconv_mime_encode('Subject', 'Prüfung', ['scheme' => 'Q', 'output-charset' => 'ISO-8859-1']), '|',
str_replace(\"\\n\", '/', iconv_mime_encode('Subject', 'ab ab ab ab',
['line-length' => 30, 'line-break-chars' => \"\\n\"]));",
    );
    assert_eq!(
        out,
        "Subject: =?UTF-8?Q?Pr=C3=BCfung?=|Subject: =?ISO-8859-1?Q?Pr=FCfung?=|\
         Subject: =?UTF-8?B?YWI=?=/ =?UTF-8?B?IGFiIGFiIGE=?=/ =?UTF-8?B?Yg==?="
    );
}

/// Verifies `iconv_mime_decode()` decodes one field and joins adjacent encoded-words.
#[test]
fn test_iconv_mime_decode_reads_one_field() {
    let out = compile_and_run(
        "<?php echo iconv_mime_decode('Subject: =?ISO-8859-1?Q?Pr=FCfung?='), '|',
iconv_mime_decode('=?UTF-8?Q?a?= =?UTF-8?Q?b?='), '|',
iconv_mime_decode(\"Subject: a\\r\\nFrom: b\");",
    );
    assert_eq!(out, "Subject: Prüfung|ab|Subject: a");
}

/// Verifies the two decode modes select php-src's strict and lenient behaviors.
#[test]
fn test_iconv_mime_decode_modes() {
    let out = compile_and_run(
        "<?php echo iconv_mime_decode('=?UTF-8?X?zz?=', ICONV_MIME_DECODE_CONTINUE_ON_ERROR), '|',
iconv_mime_decode('=?UTF-8?Q?a?=x', ICONV_MIME_DECODE_STRICT), '|',
iconv_mime_decode('=?UTF-8?Q?a?=x');",
    );
    assert_eq!(out, "=?UTF-8?X?zz?=|=?UTF-8?Q?a?=x|ax");
}

/// Verifies `iconv_mime_decode()` fails with a warning when a word is malformed.
#[test]
fn test_iconv_mime_decode_reports_malformed_words() {
    let out = compile_and_run_capture("<?php var_dump(iconv_mime_decode('=?UTF-8?X?zz?='));");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
    assert!(
        out.stderr
            .contains("Warning: iconv_mime_decode(): Malformed string"),
        "missing malformed-string warning: {}",
        out.stderr
    );
}

/// Verifies `iconv_mime_decode_headers()` builds an array and lists repeated field names.
#[test]
fn test_iconv_mime_decode_headers_builds_an_array() {
    let out = compile_and_run(
        "<?php $headers = iconv_mime_decode_headers(
\"Subject: =?ISO-8859-1?Q?Pr=FCfung?=\\r\\nTo: a@b.c\\r\\nTo: d@e.f\\r\\n\\r\\nbody\");
echo $headers['Subject'], '|', count($headers['To']), '|', $headers['To'][0], '|',
$headers['To'][1], '|', count($headers);",
    );
    assert_eq!(out, "Prüfung|2|a@b.c|d@e.f|2");
}

/// Verifies a folded continuation line joins into the field it continues.
#[test]
fn test_iconv_mime_decode_headers_joins_folded_lines() {
    let out = compile_and_run(
        "<?php $headers = iconv_mime_decode_headers(\"A: 1\\r\\n 2\\r\\nB: 3\");
echo $headers['A'], '|', $headers['B'];",
    );
    assert_eq!(out, "1 2|3");
}

/// Verifies the encoding trio starts at UTF-8 and follows `iconv_set_encoding()`.
#[test]
fn test_iconv_encoding_trio_is_configurable() {
    let out = compile_and_run(
        "<?php $all = iconv_get_encoding();
echo $all['input_encoding'], '|', $all['output_encoding'], '|', $all['internal_encoding'], '|',
var_export(iconv_set_encoding('internal_encoding', 'ISO-8859-1'), true), '|',
iconv_get_encoding('internal_encoding'), '|', iconv_strlen('héllo'), '|',
var_export(iconv_set_encoding('bogus', 'UTF-8'), true), '|',
var_export(iconv_get_encoding('bogus'), true);",
    );
    assert_eq!(out, "UTF-8|UTF-8|UTF-8|true|ISO-8859-1|6|false|false");
}

/// Verifies the extension's four constants carry PHP's values and elephc's provider names.
///
/// `ICONV_IMPL` follows the compilation target because Apple platforms ship GNU libiconv
/// while elephc's Linux support targets glibc.
#[test]
fn test_iconv_constants() {
    let out = compile_and_run(
        "<?php echo ICONV_MIME_DECODE_STRICT, '|', ICONV_MIME_DECODE_CONTINUE_ON_ERROR, '|',
ICONV_IMPL, '|', ICONV_VERSION;",
    );
    let implementation = if cfg!(target_os = "macos") {
        "libiconv"
    } else {
        "glibc"
    };
    assert_eq!(out, format!("1|2|{implementation}|unknown"));
}

/// Verifies `iconv_mime_encode()` honors its options through every receiver shape.
///
/// A receiver whose static type is `mixed` reaches the backend as a boxed cell rather than
/// a hash pointer, and silently fell back to the defaults before it was unboxed at the
/// call site.
#[test]
fn test_iconv_mime_encode_options_receiver_shapes() {
    let out = compile_and_run(
        "<?php function options(): mixed { return ['scheme' => 'Q']; }
$variable = ['scheme' => 'Q'];
$dynamic = [];
$dynamic['scheme'] = 'Q';
$boxed = options();
$nested = ['inner' => ['scheme' => 'Q']];
echo iconv_mime_encode('S', 'Prüfung', ['scheme' => 'Q']), '|',
iconv_mime_encode('S', 'Prüfung', $variable), '|',
iconv_mime_encode('S', 'Prüfung', $dynamic), '|',
iconv_mime_encode('S', 'Prüfung', options()), '|',
iconv_mime_encode('S', 'Prüfung', $boxed), '|',
iconv_mime_encode('S', 'Prüfung', $nested['inner']), '|',
iconv_mime_encode('S', 'Prüfung', []);",
    );
    let quoted = "S: =?UTF-8?Q?Pr=C3=BCfung?=";
    assert_eq!(
        out,
        format!("{quoted}|{quoted}|{quoted}|{quoted}|{quoted}|{quoted}|S: =?UTF-8?B?UHLDvGZ1bmc=?=")
    );
}

/// Verifies every iconv builtin works through PHP's first-class callable syntax.
///
/// The callable wrapper reads the shared contract's declared return type rather than a
/// per-call-site checked type, so a union-returning builtin has to declare `mixed` there;
/// declaring the narrow type instead handed the caller a raw pointer. The wrapper also
/// references the bridge entry points without any direct call recording the requirement,
/// which is what forces the bridge to link.
#[test]
fn test_iconv_first_class_callables() {
    let out = compile_and_run(
        "<?php $length = iconv_strlen(...); $convert = iconv(...); $slice = iconv_substr(...);
$find = iconv_strpos(...); $rfind = iconv_strrpos(...); $encode = iconv_mime_encode(...);
$decode = iconv_mime_decode(...); $headers = iconv_mime_decode_headers(...);
$get = iconv_get_encoding(...); $set = iconv_set_encoding(...);
echo $length('héllo'), '|', $convert('UTF-8', 'ASCII//TRANSLIT', 'café'), '|',
$slice('héllo', 1, 3), '|', $find('héllo', 'l'), '|',
var_export($find('héllo', 'z'), true), '|', $rfind('abcabc', 'bc'), '|',
$encode('Subject', 'Prüfung'), '|', $decode('=?UTF-8?Q?a?='), '|',
$headers('A: 1')['A'], '|', $get('internal_encoding'), '|',
var_export($set('internal_encoding', 'UTF-8'), true);",
    );
    // `//TRANSLIT` spells its approximation the way the platform's iconv does; see
    // `test_iconv_supports_translit_and_ignore_suffixes`.
    let cafe = if cfg!(target_os = "macos") { "caf'e" } else { "cafe" };
    assert_eq!(
        out,
        format!("5|{cafe}|éll|2|false|4|Subject: =?UTF-8?B?UHLDvGZ1bmc=?=|a|1|UTF-8|true")
    );
}
