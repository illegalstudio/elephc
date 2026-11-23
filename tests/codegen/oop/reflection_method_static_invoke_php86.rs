//! Purpose:
//! Runs PHP 8.6's ReflectionMethod static-invocation object deprecation fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_method_static_invoke_php86`.
//!
//! Key details:
//! - PHP 8.6 deprecates passing an object to `invoke()`/`invokeArgs()` when the
//!   reflected method is static; `null` (the expected receiver) stays silent.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with merged stderr/stdout on the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with merged stderr/stdout on the PHP 8.5 profile.
fn run85(source: &str) -> String {
    compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php85)
}

/// Verifies the static-method object deprecation fires for both invoke forms on 8.6.
#[test]
fn test_reflection_method_static_object_deprecation_php86() {
    let source = r#"<?php
class A { public static function s(int $v = 5): int { return $v; } }
$rm = new ReflectionMethod('A', 's');
$obj = new A();
echo $rm->invoke($obj), '|';
echo $rm->invokeArgs($obj, [7]), '|';
echo $rm->invoke(null);
"#;
    let out86 = run86(source);
    assert!(
        out86.contains(
            "Deprecated: Calling ReflectionMethod::invoke() for static method A::s() does not need an object parameter"
        ),
        "{out86}"
    );
    assert!(
        out86.contains(
            "Deprecated: Calling ReflectionMethod::invokeArgs() for static method A::s() does not need an object parameter"
        ),
        "{out86}"
    );
    // The deprecations interleave with stdout, so the values are checked piecewise.
    assert!(out86.contains("5|"), "{out86}");
    assert!(out86.contains("7|5"), "{out86}");

    let out85 = run85(source);
    assert!(!out85.contains("Deprecated:"), "{out85}");
    assert!(out85.ends_with("5|7|5"), "{out85}");
}

/// Verifies a null receiver for a static method stays silent on 8.6.
#[test]
fn test_reflection_method_static_null_receiver_stays_silent_php86() {
    let source = r#"<?php
class A { public static function s(): int { return 5; } }
$rm = new ReflectionMethod('A', 's');
echo $rm->invoke(null), '|';
echo $rm->invokeArgs(null, []);
"#;
    let out = run86(source);
    assert!(!out.contains("Deprecated:"), "{out}");
    assert!(out.ends_with("5|5"), "{out}");
}
