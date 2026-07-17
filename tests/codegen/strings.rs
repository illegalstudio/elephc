//! Purpose:
//! Groups the strings integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for search, transform, encoding, formatting, interpolation and hashes, and related suites.

use crate::support::*;

#[path = "strings/search.rs"]
mod search;
#[path = "strings/transform.rs"]
mod transform;
#[path = "strings/encoding.rs"]
mod encoding;
#[path = "strings/formatting.rs"]
mod formatting;
#[path = "strings/interpolation_and_hashes.rs"]
mod interpolation_and_hashes;
#[path = "strings/misc.rs"]
mod misc;

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
