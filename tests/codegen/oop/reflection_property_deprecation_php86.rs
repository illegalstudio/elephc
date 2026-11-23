//! Purpose:
//! Runs PHP 8.6's ReflectionProperty unrelated-object deprecation fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests reflection_property_deprecation_php86`.
//!
//! Key details:
//! - PHP 8.6 deprecates `setValue()`/`setRawValue()` when the given object is not
//!   an instance of the class the property was declared in. The synthetic
//!   `setValue()` body asks the declaring class's reflector, so the check uses
//!   the same class relation as the rest of Reflection.

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

/// Verifies the unrelated-object deprecation fires on 8.6 and stays silent on 8.5.
#[test]
fn test_reflection_property_unrelated_object_deprecation_php86() {
    let source = r#"<?php
class A { public int $x = 0; }
class B { public int $x = 0; }
$rp = new ReflectionProperty('A', 'x');
$b = new B();
$rp->setValue($b, 42);
echo $b->x;
"#;
    let out86 = run86(source);
    assert!(
        out86.contains("Deprecated: Calling ReflectionProperty::setValue() with a given object that is not an instance of the class this property was declared in is deprecated"),
        "{out86}"
    );
    assert!(out86.ends_with("42"), "{out86}");

    let out85 = run85(source);
    assert!(
        !out85.contains("Deprecated: Calling ReflectionProperty::setValue()"),
        "{out85}"
    );
    assert!(out85.ends_with("42"), "{out85}");
}

/// Verifies a related object stays silent on 8.6.
#[test]
fn test_reflection_property_related_object_stays_silent_php86() {
    let source = r#"<?php
class A { public int $x = 0; }
$a = new A();
$rp = new ReflectionProperty('A', 'x');
$rp->setValue($a, 7);
echo $a->x;
"#;
    let out = run86(source);
    assert!(!out.contains("Deprecated:"), "{out}");
    assert!(out.ends_with("7"), "{out}");
}
