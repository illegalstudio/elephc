//! Purpose:
//! Protects argument validation and literal character semantics in mbstring fast paths.
//!
//! Called from:
//! - The codegen string integration suite.
//!
//! Key details:
//! - Every fixture runs with EIR optimization enabled and disabled.
//! - Heap diagnostics cover the bridge fallbacks and caught exception cleanup.

use crate::support::*;

/// Checks output and ownership with both implementations of the length observers.
fn assert_mb_fast_semantics(source: &str, expected: &str) {
    let check = || {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    };
    check();
    without_ir_opt(check);
}

/// Exercises INI wrappers through the CLI pipeline, with managed regex and heap verification.
fn assert_mb_fast_cli_semantics(source: &str, expected: &str) {
    for mode in ["--ir-opt=on", "--ir-opt=off"] {
        let dir = make_cli_test_dir("elephc_mb_fast_request_state");
        let checked = std::panic::catch_unwind(|| {
            let path = dir.join("main.php");
            std::fs::write(&path, source).unwrap();
            let compile = elephc_cli_command_with_oniguruma(&dir)
                .args([mode, "--heap-debug", "--quiet"]).arg(&path).output().unwrap();
            assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
            let out = std::process::Command::new(path.with_extension("")).output().unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "{stderr}");
            assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
            assert!(!stderr.contains("Warning:"), "{stderr}");
        });
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(error) = checked { std::panic::resume_unwind(error); }
    }
}

/// Length-only observations must retain the producer's argument errors and evaluation order.
#[test]
fn test_mb_fast_semantics_length_validation() {
    assert_mb_fast_semantics(r#"<?php
function observed_mb_mode(int $mode): int { echo "side|"; return $mode; }
$s = "hello";
try { echo strlen(mb_str_pad($s, 10, "", STR_PAD_RIGHT, "UTF-8")); }
catch (ValueError $e) { echo "empty|"; }
try { echo strlen(mb_str_pad($s, 10, "-", 999, "UTF-8")); }
catch (ValueError $e) { echo "pad-mode|"; }
try { echo strlen(mb_convert_case($s, 999, "UTF-8")); }
catch (ValueError $e) { echo "case-mode|"; }
$bad_mode = "nope";
try { echo strlen(mb_convert_case($s, $bad_mode, "UTF-8")); }
catch (TypeError $e) { echo "case-type|"; }
try { echo strlen(mb_convert_case($s, observed_mb_mode(999), "UTF-8")); }
catch (ValueError $e) { echo "case-side|"; }
try { echo strlen(mb_str_pad($s, 10, "-", observed_mb_mode(999), "UTF-8")); }
catch (ValueError $e) { echo "pad-side|"; }
echo strlen(mb_str_pad($s, 3, "", 999, "UTF-8")), "|";
echo strlen(mb_str_pad($s, 10, "-", STR_PAD_BOTH, "UTF-8")), "|";
echo strlen(mb_convert_case($s, MB_CASE_FOLD_SIMPLE, "UTF-8")), "\n";
"#, "empty|pad-mode|case-mode|case-type|side|case-side|side|pad-side|5|10|5\n");
}

/// Shared byte-padding guards must identify the multibyte caller in caught errors.
#[test]
fn test_mb_fast_semantics_padding_error_names() {
    assert_mb_fast_semantics(r#"<?php
$s = "hello";
try { echo mb_str_pad($s, 10, "", STR_PAD_RIGHT, "UTF-8"); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { echo mb_str_pad($s, 10, "-", 999, "UTF-8"); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { echo str_pad($s, 10, "", STR_PAD_RIGHT); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { echo str_pad($s, 10, "-", 999); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
"#, concat!(
    "mb_str_pad(): Argument #3 ($pad_string) must not be empty\n",
    "mb_str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH\n",
    "str_pad(): Argument #3 ($pad_string) must not be empty\n",
    "str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH\n",
));
}

/// Multibyte trim masks use literal membership, including runtime dots and decreasing ranges.
#[test]
fn test_mb_fast_semantics_literal_trim_masks() {
    assert_mb_fast_semantics(r#"<?php
$s = "ab c";
$v = mb_trim($s, "a..c", "UTF-8");
echo "[", $v, "]|", strlen($v), "|", strlen(mb_trim($s, "a..c", "UTF-8")), "\n";
echo "[", mb_trim("abcba", "a..c", "UTF-8"), "]|";
echo "[", mb_ltrim("bc", "a..c", "UTF-8"), "]|";
echo "[", mb_rtrim("cb", "a..c", "UTF-8"), "]\n";
$mask = $argc > 0 ? "z..a" : "a..z";
echo "[", mb_trim("za", $mask, "UTF-8"), "]|";
echo "[", mb_ltrim("za", $mask, "UTF-8"), "]|";
echo "[", mb_rtrim("za", $mask, "UTF-8"), "]\n";
echo "[", mb_trim(".b.", ".", "UTF-8"), "]|";
echo "[", mb_trim("  hi  ", " ", "UTF-8"), "]|";
echo "[", mb_trim("abc", "", "UTF-8"), "]|";
echo "[", mb_trim("éabcé", "é", "UTF-8"), "]\n";
"#, "[b ]|2|2\n[bcb]|[bc]|[cb]\n[]|[]|[]\n[b]|[hi]|[abc]|[abc]\n");
}

/// The literal regex shortcut must match only at the beginning of the subject.
#[test]
fn test_mb_fast_semantics_regex_start_anchor() {
    assert_mb_fast_semantics(r#"<?php
echo mb_ereg_match("[0-9A-Za-z]+", "!!!a") ? "1" : "0", "|";
echo mb_ereg_match("[0-9A-Za-z]+", "a!!!") ? "1" : "0", "|";
echo mb_ereg_match("[0-9A-Za-z]+", "") ? "1" : "0", "|";
echo mb_ereg_match("[0-9A-Za-z]+", "abc123") ? "1" : "0", "\n";
"#, "0|1|0|1\n");
}

/// MIME output bytes follow each live destination encoding for literals and dynamic sources.
#[test]
fn test_mb_fast_semantics_mime_request_encoding() {
    assert_mb_fast_semantics(r#"<?php
foreach (["ISO-8859-1", "UTF-16LE", "UTF-8"] as $encoding) {
    mb_internal_encoding($encoding);
    echo bin2hex(mb_decode_mimeheader("=?UTF-8?B?w6k=?=")), "|";
    $word = $argc > 0 ? "=?UTF-8?B?w6k=?=" : "";
    echo bin2hex(mb_decode_mimeheader($word)), "\n";
}
"#, "e9|e9\ne900|e900\nc3a9|c3a9\n");
}

/// Every literal regex shortcut must honor the request encoding and subsequent changes.
#[test]
fn test_mb_fast_semantics_regex_request_encoding() {
    assert_mb_fast_semantics(r#"<?php
foreach (["UTF-16LE", "UTF-8"] as $encoding) {
    mb_regex_encoding($encoding);
    echo mb_ereg("[0-9]+", "1234") ? "1" : "0", "|";
    echo mb_ereg_match("[0-9A-Za-z]+", "ab") ? "1" : "0", "|";
    echo mb_eregi("[A-Z]+", "abcd") ? "1" : "0", "|";
    echo bin2hex(mb_eregi_replace("[A-Z]+", "X", "abcd")), "\n";
}
"#, "0|0|0|61626364\n1|1|1|58\n");
}

/// Length/count observers preserve non-default syntax, multibyte delimiters and later resets.
#[test]
fn test_mb_fast_semantics_regex_observer_state() {
    assert_mb_fast_cli_semantics(r##"<?php
mb_regex_encoding("UTF-16LE");
echo strlen(mb_ereg_replace("[0-9]+", "#", "1234")), "|";
mb_regex_encoding("UTF-8");
mb_regex_set_options("b");
echo strlen(mb_ereg_replace("[0-9]+", "#", "1234")), "|";
mb_regex_set_options("pr");
echo strlen(mb_ereg_replace("[0-9]+", "#", "1234")), "\n";
mb_regex_encoding("SJIS");
echo count(mb_split("@", "\x81\x40")), "|";
mb_regex_encoding("UTF-8");
echo count(mb_split("@", "a@b")), "|", count(mb_split("@", "©@x")), "\n";
ini_set("mbstring.regex_retry_limit", "1");
set_error_handler(function ($number, $message) { echo "warning|"; return true; });
echo strlen(mb_ereg_replace("[0-9]+", "#", "1234")), "\n";
mb_ereg_search_init("1234", "[0-9]+");
echo mb_ereg_search() ? "1" : "0", "\n";
ini_restore("mbstring.regex_retry_limit");
try { echo count(mb_split("@", "\xff")); }
catch (TypeError $e) { echo "count-error\n"; }
restore_error_handler();
"##, "4|4|1\n1|2|2\nwarning|0\nwarning|0\ncount-error\n");
}

/// Callees retain progressive mutations and read encoding settings established by their caller.
#[test]
fn test_mb_fast_semantics_caller_session_state() {
    assert_mb_fast_cli_semantics(r#"<?php
function inactiveGlobalProbe(): int { return 0; }
function callbackProgressivePosition(): int {
    mb_ereg_search_init("abc123", "[0-9]+"); mb_ereg_search();
    return mb_ereg_search_getpos();
}
function callbackProgressiveArray(): int {
    mb_ereg_search_init("abc123", "[0-9]+"); $r=mb_ereg_search_pos();
    return $r[0];
}
function callbackIconvWidth(): int { return strlen((string)iconv_get_encoding("internal_encoding")); }
mb_ereg_search_init("seed00777", "[0-9]+"); mb_ereg_search();
$callback=$argc>0?"callbackProgressivePosition":"inactiveGlobalProbe";
echo call_user_func($callback),"|",mb_ereg_search_getpos(),"\n";
mb_ereg_search_init("seed00777", "[0-9]+"); mb_ereg_search();
$callback=$argc>0?"callbackProgressiveArray":"inactiveGlobalProbe";
echo call_user_func($callback),"|",mb_ereg_search_getpos(),"\n";
set_error_handler(function($n,$m){return true;});
iconv_set_encoding("internal_encoding", "ISO-8859-1");
restore_error_handler();
$callback=$argc>0?"callbackIconvWidth":"inactiveGlobalProbe";
echo call_user_func($callback),"\n";
"#, "6|6\n3|6\n10\n");
}

/// Unobserved converted arrays can still update the request's illegal-character count.
#[test]
fn test_mb_fast_semantics_conversion_state() {
    assert_mb_fast_semantics(r#"<?php
$items = ["\xff"];
echo strlen((string)mb_convert_variables("UTF-8", "UTF-8", $items)), "|";
echo mb_get_info("illegal_chars"), "\n";
"#, "5|1\n");
}

/// Result-returning progressive searches preserve live limits, captures and warning delivery.
#[test]
fn test_mb_fast_semantics_progressive_array_limits() {
    assert_mb_fast_cli_semantics(r#"<?php
set_error_handler(function ($number, $message) { echo "warning|"; return true; });
mb_ereg_search_init("1234", "[0-9]+");
mb_ereg_search();
mb_ereg_search_setpos(0);
ini_set("mbstring.regex_retry_limit", "1");
echo mb_ereg_search_pos() === false ? "false|" : "match|";
echo mb_ereg_search_getpos(), "|", mb_ereg_search_getregs() === false ? "empty" : "matched", "\n";
ini_restore("mbstring.regex_retry_limit");
mb_ereg_search_init("1234", "[0-9]+");
mb_ereg_search();
mb_ereg_search_setpos(0);
ini_set("mbstring.regex_retry_limit", "1");
echo mb_ereg_search_regs() === false ? "false|" : "match|";
echo mb_ereg_search_getpos(), "|", mb_ereg_search_getregs() === false ? "empty" : "matched", "\n";
ini_restore("mbstring.regex_retry_limit");
mb_ereg_search_init("ab123", "[0-9]+");
$position = mb_ereg_search_pos();
echo $position[0], "|", $position[1], "\n";
mb_ereg_search_init("ab123", "[0-9]+");
$registers = mb_ereg_search_regs();
echo $registers[0], "\n";
restore_error_handler();
"#, "warning|false|0|empty\nwarning|false|0|empty\n2|3\n123\n");
}

/// Padding fast paths retain argument TypeErrors while weak numeric strings use the bridge.
#[test]
fn test_mb_fast_semantics_padding_type_validation() {
    assert_mb_fast_semantics(r#"<?php
$invalid = $argc > 0 ? "nope" : "0";
try { echo mb_str_pad("hello", 10, "-", $invalid, "UTF-8"); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { echo mb_str_pad("hello", $invalid, "-", STR_PAD_RIGHT, "UTF-8"); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { echo mb_str_pad("hello", 3, "", $invalid, "UTF-8"); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$width = $argc > 0 ? "8" : "7";
$mode = $argc > 0 ? "0" : "1";
echo mb_str_pad("hello", $width, "-", $mode, "UTF-8"), "\n";
"#, "mb_str_pad(): Argument #4 ($pad_type) must be of type int, string given\nmb_str_pad(): Argument #2 ($length) must be of type int, string given\nmb_str_pad(): Argument #4 ($pad_type) must be of type int, string given\n---hello\n");
}
