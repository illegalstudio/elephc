//! Purpose:
//! Runs php-src 8.6's `#[\Override]` class-constant fixture through the native backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests override_constants_php86`.
//!
//! Key details:
//! - Source and expected output are transcribed from
//!   `Zend/tests/attributes/override/constants/basic.phpt`.
//! - The fixture covers classes, interfaces, enums, and an enum case whose name matches an
//!   implemented-interface constant.

use crate::support::*;

/// Accepts `#[\Override]` on constants and enum cases that have a matching parent declaration.
#[test]
fn test_override_constants_basic_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

interface I {
    public const I = 'I';
}

interface II extends I {
    #[\Override]
    public const I = 'I';
}

class P {
    public const C1 = 'C1';
    public const C2 = 'C2';
    public const C3 = 'C3';
    public const C4 = 'C4';
}

class PP extends P {
    #[\Override]
    public const C1 = 'C1';
    public const C2 = 'C2';
    #[\Override]
    public const C3 = 'C3';
}

class C extends PP implements I {
    #[\Override]
    public const I = 'I';
    #[\Override]
    public const C1 = 'C1';
    #[\Override]
    public const C2 = 'C2';
    public const C3 = 'C3';
    #[\Override]
    public const C4 = 'C4';
    public const C = 'C';
}

enum E implements I {
    #[\Override]
    public const I = 'I';
}

enum WithCase implements I {
    #[\Override]
    case I;
}

echo "Done";

"#);
    assert_eq!(out.trim(), "Done");
}

/// php-src `trait_parent.phpt`.
#[test]
fn test_override_constants_trait_parent_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

trait DemoTrait {
    #[\Override]
    public const C = 'Changed';
}

class Base {
    public const C = 'C';
}

class UsesTrait extends Base {
    use DemoTrait;
}

echo "Done";

"#);
    assert_eq!(out.trim(), "Done");
}

/// php-src `trait_unused.phpt`.
#[test]
fn test_override_constants_trait_unused_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

trait Demo {
    #[\Override]
    public const T = 'T';
}

echo "Done";

"#);
    assert_eq!(out.trim(), "Done");
}

/// php-src `trait_interface.phpt`.
#[test]
fn test_override_constants_trait_interface_php86_php_src_fixture() {
    let out = compile_and_run(r#"<?php

trait DemoTrait {
    #[\Override]
    public const C = 'Changed';
}

interface IFace {
    public const C = 'C';
}

class UsesTrait implements IFace {
    use DemoTrait;
}

echo "Done";

"#);
    assert_eq!(out.trim(), "Done");
}
