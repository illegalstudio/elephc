//! Purpose:
//! Verifies PHP 8.6's form-feed addition to the default trim character list.
//!
//! Called from:
//! - `cargo test --test codegen_tests trim_php86`.
//!
//! Key details:
//! - PHP 8.6 adds `\x0C` (form feed) to the default masks of `trim()`, `ltrim()`,
//!   `rtrim()` and `chop()`; earlier profiles leave it untouched.
//! - Explicit character lists never change, on either profile.
//! - Expected outputs are byte-exact hexadecimal renderings of the PHPT-observable
//!   result, so a stray space or NUL cannot pass unnoticed.

use crate::support::*;

/// PHP 8.5 and earlier do not treat form feed as trimmable by default.
#[test]
fn test_trim_default_mask_leaves_form_feed_on_php85() {
    let source = r#"<?php
$s = "\x0C \tABC \x0C";
echo bin2hex(trim($s)), "|", bin2hex(ltrim($s)), "|", bin2hex(rtrim($s)), "|", bin2hex(chop($s));
echo "|", bin2hex(trim($s, " \t")), "|", bin2hex(ltrim($s, " \t")), "|", bin2hex(rtrim($s, " \t"));
"#;
    assert_eq!(
        compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php85),
        "0c2009414243200c|0c2009414243200c|0c2009414243200c|0c2009414243200c|\
         0c2009414243200c|0c2009414243200c|0c2009414243200c"
    );
}

/// PHP 8.6 trims form feed with the default character list.
#[test]
fn test_trim_default_mask_includes_form_feed_on_php86() {
    let source = r#"<?php
$s = "\x0C \tABC \x0C";
echo bin2hex(trim($s)), "|", bin2hex(ltrim($s)), "|", bin2hex(rtrim($s)), "|", bin2hex(chop($s));
echo "|", bin2hex(trim($s, " \t")), "|", bin2hex(ltrim($s, " \t")), "|", bin2hex(rtrim($s, " \t"));
"#;
    assert_eq!(
        compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php86),
        "414243|414243200c|0c2009414243|0c2009414243|\
         0c2009414243200c|0c2009414243200c|0c2009414243200c"
    );
}

/// An explicit character list that names form feed behaves identically on both profiles.
#[test]
fn test_trim_explicit_form_feed_charlist_is_version_independent() {
    let source = r#"<?php
$s = "\x0C \tABC \x0C";
echo bin2hex(trim($s, "\x0C")), "|", bin2hex(ltrim($s, "\x0C")), "|", bin2hex(rtrim($s, "\x0C"));
"#;
    for profile in [
        elephc::php_version::PhpVersion::Php85,
        elephc::php_version::PhpVersion::Php86,
    ] {
        assert_eq!(
            compile_and_run_with_php_version(source, profile),
            "200941424320|2009414243200c|0c200941424320"
        );
    }
}
