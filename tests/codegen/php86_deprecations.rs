//! Purpose:
//! PHP 8.6 runtime deprecations from the builtin constant table, compiled under an explicit
//! compatibility profile.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` binary (`tests/codegen/system.rs`).
//!
//! Key details:
//! - `SORT_LOCALE_STRING` becomes deprecated in 8.6; the read reports php-src's message once
//!   and still evaluates to its integer value. The 8.5 profile stays silent.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Reading `SORT_LOCALE_STRING` reports the 8.6 deprecation and keeps its value.
#[test]
fn test_sort_locale_string_read_is_deprecated_php86() {
    let source = "<?php $flags = SORT_LOCALE_STRING; echo $flags;";
    let out = compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php86);
    assert!(
        out.contains("Constant SORT_LOCALE_STRING is deprecated since 8.6"),
        "{out}"
    );
    assert!(out.contains('5'), "{out}");

    let older = compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php85);
    assert!(
        !older.contains("SORT_LOCALE_STRING is deprecated"),
        "PHP 8.5 must not deprecate the flag: {older}"
    );
    assert!(older.contains('5'), "{older}");
}
