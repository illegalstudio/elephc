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

/// Enum engine interfaces reject ordinary classes and explicit implementation by enums.
#[test]
fn test_core_symbols_enum_interface_misuse() {
    expect_error("<?php class Bad implements UnitEnum { public static function cases(): array { return []; } }", "Non-enum class Bad cannot implement interface UnitEnum");
    // PHP 8.5: "Enum Bad cannot implement previously implemented interface UnitEnum".
    expect_error("<?php enum Bad implements UnitEnum { case One; }", "Enum Bad cannot implement previously implemented interface UnitEnum");
}

/// Core internal attributes validate declaration targets and repeatability.
#[test]
fn test_core_symbols_attribute_target_errors() {
    expect_error("<?php #[SensitiveParameter] class Bad {}", "cannot target class");
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
