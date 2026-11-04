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
