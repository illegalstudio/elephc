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

/// PHP 8.6's declaration-time deprecations run once at startup and stay silent on 8.5.
#[test]
fn test_declaration_deprecations_php86() {
    let source = r#"<?php
class A {
    const namespace = 1;
    public function __debugInfo(): ?array { return null; }
}
function readonly() {}
const let = 5;
class is {}
echo A::namespace + let;
"#;
    let out = compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php86);
    for needle in [
        "Declaring a nullable return type for A::__debugInfo() is deprecated",
        "Declaring a class constant named namespace is deprecated",
        "Declaring a function named readonly is deprecated",
        "Using let as a constant name is deprecated",
        "Using is as a class name is deprecated",
    ] {
        assert!(out.contains(needle), "missing {needle:?} in {out}");
    }

    let older = compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php85);
    assert!(
        !older.contains("Deprecated:"),
        "PHP 8.5 must not report 8.6 declaration deprecations: {older}"
    );
    assert!(older.contains('6'), "{older}");
}
