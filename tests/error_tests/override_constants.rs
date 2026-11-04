//! Purpose:
//! Checks PHP 8.6's `#[\Override]` class-constant failure diagnostics.
//!
//! Called from:
//! - `cargo test --test error_tests override_constants`.
//!
//! Key details:
//! - Sources are transcribed from
//!   `Zend/tests/attributes/override/constants/*_failure.phpt` (PHP-8.6 branch).
//! - php-src reports a fatal error at declaration time; elephc reports the same rule while
//!   compiling.

use super::*;

/// php-src `failure.phpt`.
#[test]
fn test_override_constants_failure_php86() {
    expect_error(
        r#"<?php

class Demo {
    #[\Override]
    public const C = 'C';
}

echo "Done";

"#,
        "has #[\\Override] attribute, but no matching parent constant exists",
    );
}

/// php-src `interface_failure.phpt`.
#[test]
fn test_override_constants_interface_failure_php86() {
    expect_error(
        r#"<?php

interface IFace {
    #[\Override]
    public const I = 'I';
}

echo "Done";

"#,
        "has #[\\Override] attribute, but no matching parent constant exists",
    );
}

/// php-src `enum_failure.phpt`.
#[test]
fn test_override_constants_enum_failure_php86() {
    expect_error(
        r#"<?php

enum Demo {
    #[\Override]
    public const C = 'C';
}

echo "Done";

"#,
        "has #[\\Override] attribute, but no matching parent constant exists",
    );
}

/// php-src `anon_failure.phpt`.
#[test]
fn test_override_constants_anon_failure_php86() {
    expect_error(
        r#"<?php

new class () {
    #[\Override]
    public const C = 'C';
};

echo "Done";

"#,
        "has #[\\Override] attribute, but no matching parent constant exists",
    );
}

/// php-src `enum_failure_case.phpt`.
#[test]
fn test_override_constants_enum_failure_case_php86() {
    expect_error(
        r#"<?php

enum Demo {
    #[\Override]
    case C;
}

echo "Done";

"#,
        "has #[\\Override] attribute, but no matching parent constant exists",
    );
}

/// php-src `trait_failure.phpt`.
#[test]
fn test_override_constants_trait_failure_php86() {
    expect_error(
        r#"<?php

trait DemoTrait {
    #[\Override]
    public const T = 'T';
}

class UsesTrait {
    use DemoTrait;
}

echo "Done";

"#,
        "has #[\\Override] attribute, but no matching parent constant exists",
    );
}
