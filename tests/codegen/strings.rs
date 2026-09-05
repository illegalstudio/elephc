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

/// Verifies `mb_convert_case()` title case, including Termwind's `MB_CASE_TITLE` + UTF-8 path.
#[test]
fn test_mb_convert_case_title() {
    let out = compile_and_run(
        r#"<?php
echo mb_convert_case("hello world", MB_CASE_TITLE), ":";
echo mb_convert_case("don't stop", MB_CASE_TITLE), ":";
echo mb_convert_case("mary had a Little lamb", MB_CASE_TITLE, "UTF-8"), ":";
echo mb_convert_case("héllo", MB_CASE_TITLE), ":";
echo mb_convert_case(string: "hi", mode: MB_CASE_TITLE, encoding: null);"#,
    );
    assert_eq!(out, "Hello World:Don't Stop:Mary Had A Little Lamb:Héllo:Hi");
}

/// Verifies every `MB_CASE_*` mode, including full vs simple ß expansion.
#[test]
fn test_mb_convert_case_modes() {
    let out = compile_and_run(
        r#"<?php
echo mb_convert_case("hello", MB_CASE_UPPER), ":";
echo mb_convert_case("HELLO", MB_CASE_LOWER), ":";
echo mb_convert_case("straße", MB_CASE_UPPER), ":";
echo mb_convert_case("straße", MB_CASE_UPPER_SIMPLE), ":";
echo mb_convert_case("ß", MB_CASE_TITLE), ":";
echo mb_convert_case("ß", MB_CASE_TITLE_SIMPLE), ":";
echo mb_convert_case("Straße", MB_CASE_LOWER), ":";
echo mb_convert_case("Straße", MB_CASE_FOLD), ":";
echo mb_convert_case("Straße", MB_CASE_FOLD_SIMPLE), ":";
echo MB_CASE_TITLE, MB_CASE_UPPER, MB_CASE_LOWER;"#,
    );
    assert_eq!(out, "HELLO:hello:STRASSE:STRAßE:Ss:ß:straße:strasse:straße:201");
}

/// Verifies encoding aliases, first-class callables, and namespaced lookup.
#[test]
fn test_mb_convert_case_encoding_and_callable() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo mb_convert_case("hello", MB_CASE_UPPER, "8bit"), ":";
echo Mb_CoNvErT_cAsE("hi", MB_CASE_TITLE, "UTF8"), ":";
$fn = mb_convert_case(...);
echo $fn("abc", MB_CASE_UPPER);
$mode = $argc > 0 ? 99 : MB_CASE_TITLE;
$encoding = $argc > 0 ? "definitely-not-an-encoding" : "UTF-8";
try {
    mb_convert_case("x", $mode);
} catch (\ValueError $error) {
    echo ":mode";
}
try {
    mb_convert_case("x", MB_CASE_TITLE, $encoding);
} catch (\ValueError $error) {
    echo ":enc";
}"#,
    );
    assert_eq!(out, "HELLO:Hi:ABC:mode:enc");
}
