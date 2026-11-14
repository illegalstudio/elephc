//! Purpose:
//! Checks PHP Core class, attribute, and interface diagnostics against invalid programs.
//!
//! Called from:
//! - The focused error_tests integration binary.
//!
//! Key details:
//! - Positive counterparts exercise deferred validation and discarded-result warnings.

use super::*;

/// Tentative builtin returns do not weaken a user interface's explicitly redeclared contract.
#[test]
fn test_core_symbols_serializable_user_return_contract() {
    expect_error("<?php interface StrictPacket extends Serializable { public function serialize(): ?string; } class Bad implements StrictPacket { public function serialize() { return 'x'; } public function unserialize($data) {} }", "without declaring a compatible return type");
}

/// Issue #2007: enum interface misuse names the implementer and PHP's offending interface.
#[test]
fn test_core_symbols_enum_interface_misuse() {
    let cases = [
        (
            "<?php class C implements UnitEnum { public static function cases(): array { return []; } }",
            "Non-enum class C cannot implement interface UnitEnum",
        ),
        (
            "<?php class C implements BackedEnum { public static function cases(): array { return []; } public static function from(int|string $value): static { return new static(); } public static function tryFrom(int|string $value): ?static { return null; } }",
            "Non-enum class C cannot implement interface BackedEnum",
        ),
        (
            "<?php interface F extends BackedEnum {} class C implements F {}",
            "Non-enum class C cannot implement interface UnitEnum",
        ),
        (
            "<?php enum E implements UnitEnum { case One; }",
            "Enum E cannot implement previously implemented interface UnitEnum",
        ),
        (
            "<?php enum E: int implements BackedEnum { case One = 1; }",
            "Enum E cannot implement previously implemented interface BackedEnum",
        ),
        (
            "<?php enum E implements BackedEnum { case One; }",
            "Non-backed enum E cannot implement interface BackedEnum",
        ),
        (
            "<?php interface F extends BackedEnum {} enum E implements F { case One; }",
            "Non-backed enum E cannot implement interface BackedEnum",
        ),
        (
            "<?php enum E: int implements UnitEnum { case One = 1; }",
            "Enum E cannot implement previously implemented interface UnitEnum",
        ),
        (
            "<?php enum E: string implements BackedEnum { case One = 'one'; }",
            "Enum E cannot implement previously implemented interface BackedEnum",
        ),
        (
            "<?php interface F extends UnitEnum {} class C implements F {}",
            "Non-enum class C cannot implement interface UnitEnum",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(check_source(source).unwrap_err(), expected, "Source: {source}");
    }
}

/// Non-enum classes name the interface PHP's link order reaches first; expectations are measured.
#[test]
fn test_core_symbols_enum_interface_link_order_for_classes() {
    let cases = [
        ("class C implements UnitEnum, BackedEnum {}", "UnitEnum"),
        ("class C implements BackedEnum, UnitEnum {}", "BackedEnum"),
        ("interface F extends BackedEnum {} class C implements F, BackedEnum {}", "UnitEnum"),
        ("interface F extends BackedEnum {} class C implements BackedEnum, F {}", "BackedEnum"),
        ("interface F extends BackedEnum {} class C implements F, UnitEnum {}", "BackedEnum"),
        ("interface F extends BackedEnum {} class C implements UnitEnum, F {}", "UnitEnum"),
        ("interface F extends UnitEnum {} class C implements F, BackedEnum {}", "UnitEnum"),
        ("interface F extends UnitEnum {} class C implements BackedEnum, F {}", "BackedEnum"),
        ("interface F extends UnitEnum, BackedEnum {} class C implements F {}", "BackedEnum"),
        ("interface F extends BackedEnum, UnitEnum {} class C implements F {}", "UnitEnum"),
        ("interface F extends Countable, BackedEnum {} class C implements F {}", "UnitEnum"),
        ("interface G extends BackedEnum {} interface F extends G {} class C implements F {}", "BackedEnum"),
        ("abstract class C implements Countable, BackedEnum {}", "BackedEnum"),
    ];
    for (source, interface) in cases {
        let source = format!("<?php {source}");
        let expected = format!("Non-enum class C cannot implement interface {interface}");
        assert_eq!(check_source(&source).unwrap_err(), expected, "Source: {source}");
    }
}

/// Enums reject duplicates across `implements` and the implicit interfaces before the backing check.
#[test]
fn test_core_symbols_enum_interface_link_order_for_enums() {
    let cases = [
        (
            "enum E implements UnitEnum, BackedEnum { case One; }",
            "Enum E cannot implement previously implemented interface UnitEnum",
        ),
        (
            "enum E implements BackedEnum, UnitEnum { case One; }",
            "Enum E cannot implement previously implemented interface UnitEnum",
        ),
        (
            "enum E: int implements BackedEnum, UnitEnum { case One = 1; }",
            "Enum E cannot implement previously implemented interface UnitEnum",
        ),
        (
            "interface F extends BackedEnum {} enum E implements F, UnitEnum { case One; }",
            "Enum E cannot implement previously implemented interface UnitEnum",
        ),
        (
            "interface F extends BackedEnum {} enum E implements F, BackedEnum { case One; }",
            "Non-backed enum E cannot implement interface BackedEnum",
        ),
        (
            "interface F extends BackedEnum {} enum E: int implements F, BackedEnum { case One = 1; }",
            "Enum E cannot implement previously implemented interface BackedEnum",
        ),
        (
            "interface G extends BackedEnum {} interface F extends G {} enum E implements F { case One; }",
            "Non-backed enum E cannot implement interface BackedEnum",
        ),
    ];
    for (source, expected) in cases {
        let source = format!("<?php {source}");
        assert_eq!(check_source(&source).unwrap_err(), expected, "Source: {source}");
    }
}

/// Case declarations are reported first because PHP compiles them before linking interfaces.
#[test]
fn test_core_symbols_enum_case_errors_precede_interface_errors() {
    let cases = [
        ("enum E implements BackedEnum { case One = 1; }", "Pure enum cases cannot declare a backing value"),
        ("enum E: int implements BackedEnum { case One; }", "Backed enum cases must declare a value"),
        ("enum E implements UnitEnum { case One; case One; }", "Duplicate enum case: E::One"),
    ];
    for (source, expected) in cases {
        let source = format!("<?php {source}");
        assert_eq!(check_source(&source).unwrap_err(), expected, "Source: {source}");
    }
}

/// Namespaced implementers and imported mixed-case builtin names retain canonical diagnostics.
#[test]
fn test_core_symbols_enum_interface_misuse_namespaced() {
    let cases = [
        (
            r"<?php namespace App; use backedenum as Marker; class C implements Marker {}",
            r"Non-enum class App\C cannot implement interface BackedEnum",
        ),
        (
            r"<?php namespace App; use unitenum as Marker; enum E implements Marker { case One; }",
            r"Enum App\E cannot implement previously implemented interface UnitEnum",
        ),
        (
            r"<?php namespace App; use backedenum as Marker; enum E: int implements Marker { case One = 1; }",
            r"Enum App\E cannot implement previously implemented interface BackedEnum",
        ),
        (
            r"<?php namespace App; interface F extends \backedenum {} enum E implements F { case One; }",
            r"Non-backed enum App\E cannot implement interface BackedEnum",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(check_source(source).unwrap_err(), expected, "Source: {source}");
    }
}

/// Issue #2007's accepted enum keeps its explicit marker and implicit UnitEnum interface.
#[test]
fn test_core_symbols_enum_interface_inheritance() {
    let result = check_source_full(
        "<?php interface F extends UnitEnum {} enum E implements F { case One; }",
    ).unwrap();
    assert_eq!(result.classes["E"].interfaces, ["F", "UnitEnum"]);
    assert!(result.enums["E"].backing_type.is_none());
}

/// Core internal attributes validate declaration targets and repeatability.
#[test]
fn test_core_symbols_attribute_target_errors() {
    expect_error("<?php #[SensitiveParameter] class Bad {}", "cannot target class");
    expect_error(
        "<?php class Bad { public function __construct(#[AllowDynamicProperties] public int $x) {} }",
        "Attribute \"AllowDynamicProperties\" cannot target parameter",
    );
    // A child's plain parameter named like a parent's promoted property is not promoted.
    expect_error(
        "<?php class P { public function __construct(public int $x) {} }
class C extends P { public function __construct(#[Override] int $x) { parent::__construct($x); } }",
        "Attribute \"Override\" cannot target parameter",
    );
    expect_error("<?php #[AllowDynamicProperties] function bad(): void {}", "cannot target function");
    expect_error("<?php #[NoDiscard] class Bad {}", "cannot target class");
    expect_error("<?php #[NoDiscard] #[NoDiscard] function bad(): int { return 1; }", "must not be repeated");
    expect_error("<?php #[DelayedTargetValidation] #[DelayedTargetValidation] function bad(): void {}", "must not be repeated");
    expect_error("<?php #[Deprecated] #[Deprecated] function bad(): int { return 1; }", "must not be repeated");
    expect_error("<?php #[NoDiscard] function bad(): void {}", "NoDiscard cannot apply to a void");
}

/// The PHP 8.5 deferral marker permits an otherwise invalid Core attribute target.
#[test]
fn test_core_symbols_delayed_target_validation() {
    check_source("<?php #[DelayedTargetValidation, SensitiveParameter] class Deferred {}").unwrap();
}

/// Ignored marked returns warn while assigned returns remain accepted.
#[test]
fn test_core_symbols_no_discard_warning() {
    expect_warning("<?php #[NoDiscard] function result(): int { return 7; } result();", "The return value of result()");
    expect_no_warning("<?php #[NoDiscard] function result(): int { return 7; } $_ = result(); echo $_;", "The return value");
}

/// Override on properties requires an inherited property of the same name.
#[test]
fn test_core_symbols_property_override() {
    expect_error("<?php class Bad { #[Override] public int $value = 0; }", "no matching parent property");
    expect_error("<?php class PrivateParent { private int $value = 0; } class Bad extends PrivateParent { #[Override] public int $value = 1; }", "no matching parent property");
    check_source("<?php class ParentProperty { public int $value = 0; } class ChildProperty extends ParentProperty { #[Override] public int $value = 1; }").unwrap();
    check_source("<?php interface PropertyContract { public int $value { get; } } class ChildProperty implements PropertyContract { #[Override] public int $value = 1; }").unwrap();
}

/// New Core constructors retain normal argument-count and type diagnostics.
#[test]
fn test_core_symbols_constructor_diagnostics() {
    expect_error("<?php new SensitiveParameterValue();", "argument");
    expect_error("<?php WeakReference::create(7);", "object");
    expect_error("<?php new Attribute([]);", "int");
}
