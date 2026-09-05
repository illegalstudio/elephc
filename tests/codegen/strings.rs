//! Purpose:
//! Groups the strings integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for search, transform, encoding, iconv, formatting, interpolation and hashes, and related suites.

use crate::support::*;

#[path = "strings/search.rs"]
mod search;
#[path = "strings/transform.rs"]
mod transform;
#[path = "strings/encoding.rs"]
mod encoding;
#[path = "strings/iconv.rs"]
mod iconv;
#[path = "strings/formatting.rs"]
mod formatting;
#[path = "strings/interpolation_and_hashes.rs"]
mod interpolation_and_hashes;
#[path = "strings/misc.rs"]
mod misc;
#[path = "strings/openssl.rs"]
mod openssl;
#[path = "strings/parse_url.rs"]
mod parse_url;

/// Verifies `mb_strlen()` counts valid UTF-8 across ASCII, multibyte, and empty strings.
#[test]
fn test_mb_strlen_codepoint_count() {
    let out = compile_and_run(
        "<?php echo mb_strlen('abc'), ':', mb_strlen('héllo wörld'), ':', mb_strlen(''), ':', mb_strlen('日本語');",
    );
    assert_eq!(out, "3:11:0:3");
}

/// Verifies `mb_strlen()` accepts PHP's optional nullable encoding and byte-count aliases.
#[test]
fn test_mb_strlen_encoding_argument() {
    let out = compile_and_run(
        r#"<?php
echo mb_strlen("héllo", "UTF-8"), ":";
echo mb_strlen("héllo", "8bit"), ":";
echo mb_strlen(string: "日本語", encoding: null), ":";
$encoding = $argc > 0 ? "binary" : "UTF-8";
echo mb_strlen("héllo", $encoding), ":";
echo mb_strlen("\x68\x00\xE9\x00", "UTF-16LE"), ":";
$length = mb_strlen(...);
echo $length("héllo", "8bit");"#,
    );
    assert_eq!(out, "5:6:3:6:2:6");
}

/// Verifies malformed and truncated UTF-8 follows PHP mbstring substitution boundaries.
#[test]
fn test_mb_strlen_malformed_utf8() {
    let out = compile_and_run(
        r#"<?php
echo mb_strlen("\x80", "UTF-8"), ":";
echo mb_strlen("\xC0\xAF", "UTF-8"), ":";
echo mb_strlen("\xE2\x82", "UTF-8"), ":";
echo mb_strlen("\xED\xA0\x80", "UTF-8"), ":";
echo mb_strlen("\xF4\x90\x80\x80", "UTF-8"), ":";
echo mb_strlen("\xE2\x28\xA1", "UTF-8");"#,
    );
    assert_eq!(out, "1:2:1:3:4:3");
}

/// Verifies namespaced/case-insensitive lookup and unknown-encoding `ValueError` behavior.
#[test]
fn test_mb_strlen_namespace_and_invalid_encoding() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo Mb_StRlEn("日本語"), ":";
$encoding = $argc > 0 ? "definitely-not-an-encoding" : "UTF-8";
try {
    mb_strlen("abc", $encoding);
} catch (\ValueError $error) {
    echo "caught";
}"#,
    );
    assert_eq!(out, "3:caught");
}

/// Verifies `mb_strimwidth()` trims ASCII and CJK text by PHP display width.
#[test]
fn test_mb_strimwidth_ascii_and_cjk() {
    let out = compile_and_run(
        r#"<?php
echo mb_strimwidth("hello", 0, 3), ":";
echo mb_strimwidth("hello", 0, 3, "..."), ":";
echo mb_strimwidth("hello", 0, 4, "..."), ":";
echo mb_strimwidth("日本語", 0, 4, "…"), ":";
echo mb_strimwidth("hello", 1, 3), ":";
echo mb_strimwidth("hello", -2, 10), ":";
echo mb_strimwidth("hello", 0, -2), ":";
echo mb_strimwidth("ab", 2, 1), ":";
echo mb_strimwidth("", 0, 3, "...");"#,
    );
    assert_eq!(out, "hel:...:h...:日…:ell:lo:hel::");
}

/// Verifies `mb_strimwidth()` encoding aliases, first-class callables, and Termwind's UTF-8 form.
#[test]
fn test_mb_strimwidth_encoding_and_callable() {
    let out = compile_and_run(
        r#"<?php
echo mb_strimwidth("héllo", 0, 3, "", "UTF-8"), ":";
echo mb_strimwidth("héllo", 0, 3, "", "UTF8"), ":";
echo mb_strimwidth("héllo", 0, 3, "", "8bit"), ":";
echo mb_strimwidth(string: "日本語", start: 0, width: 4, trim_marker: "…", encoding: null), ":";
$trim = mb_strimwidth(...);
echo $trim("hello", 0, 4, "...");"#,
    );
    assert_eq!(out, "hél:hél:hé:日…:h...");
}

/// Verifies namespaced/case-insensitive lookup and catchable `mb_strimwidth()` `ValueError`s.
#[test]
fn test_mb_strimwidth_namespace_and_value_errors() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo Mb_StRiMwIdTh("日本語", 0, 4, "…"), ":";
$encoding = $argc > 0 ? "definitely-not-an-encoding" : "UTF-8";
try {
    mb_strimwidth("abc", 0, 1, "", $encoding);
} catch (\ValueError $error) {
    echo "enc";
}
echo ":";
try {
    mb_strimwidth("ab", 3, 1);
} catch (\ValueError $error) {
    echo "start";
}
echo ":";
try {
    mb_strimwidth("ab", 2, -1);
} catch (\ValueError $error) {
    echo "width";
}"#,
    );
    assert_eq!(out, "日…:enc:start:width");
}
