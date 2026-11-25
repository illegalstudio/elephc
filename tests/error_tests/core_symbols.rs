//! Purpose:
//! Checks PHP Core class, attribute, and interface diagnostics against invalid programs.
//!
//! Called from:
//! - The focused error_tests integration binary.
//!
//! Key details:
//! - Positive counterparts exercise deferred validation and discarded-result warnings.

use super::*;
use elephc::php_version::PhpVersion;

/// Tentative builtin returns do not weaken a user interface's explicitly redeclared contract.
#[test]
fn test_core_symbols_serializable_user_return_contract() {
    expect_error("<?php interface StrictPacket extends Serializable { public function serialize(): ?string; } class Bad implements StrictPacket { public function serialize() { return 'x'; } public function unserialize($data) {} }", "without declaring a compatible return type");
}

/// Enum engine interfaces reject ordinary classes and explicit implementation by enums.
#[test]
fn test_core_symbols_enum_interface_misuse() {
    expect_error("<?php class Bad implements UnitEnum { public static function cases(): array { return []; } }", "Non-enum class Bad cannot implement interface UnitEnum");
    expect_error("<?php interface Factory extends BackedEnum {} class Bad implements Factory {}", "Non-enum class Bad cannot implement interface UnitEnum");
    expect_error("<?php enum Bad implements UnitEnum { case One; }", "cannot explicitly implement");
}

/// Core target diagnostics include the full allowed-target list in PHP's order.
#[test]
fn test_core_symbols_attribute_allowed_targets() {
    for (source, expected) in [
        (
            "<?php #[SensitiveParameter] class Bad {}",
            "Attribute \"SensitiveParameter\" cannot target class (allowed targets: parameter)",
        ),
        (
            "<?php #[AllowDynamicProperties] function bad(): void {}",
            "Attribute \"AllowDynamicProperties\" cannot target function (allowed targets: class)",
        ),
        (
            "<?php class Bad { #[AllowDynamicProperties] public function bad(): void {} }",
            "Attribute \"AllowDynamicProperties\" cannot target method (allowed targets: class)",
        ),
        (
            "<?php class Bad { #[ReturnTypeWillChange] public int $value = 0; }",
            "Attribute \"ReturnTypeWillChange\" cannot target property (allowed targets: method)",
        ),
        (
            "<?php class Bad { #[Override] public const VALUE = 1; }",
            "Attribute \"Override\" cannot target class constant (allowed targets: method, property)",
        ),
        (
            "<?php class Bad { public function __construct(#[AllowDynamicProperties] public int $x) {} }",
            "Attribute \"AllowDynamicProperties\" cannot target parameter (allowed targets: class)",
        ),
        (
            "<?php function bad(#[AllowDynamicProperties] int $x): void {}",
            "Attribute \"AllowDynamicProperties\" cannot target parameter (allowed targets: class)",
        ),
        (
            "<?php #[NoDiscard] class Bad {}",
            "Attribute \"NoDiscard\" cannot target class (allowed targets: function, method)",
        ),
        (
            "<?php class Bad { #[Deprecated] public int $value = 0; }",
            "Attribute \"Deprecated\" cannot target property (allowed targets: class, function, method, class constant, constant)",
        ),
        (
            "<?php namespace App; use AllowDynamicProperties as Marker; #[Marker] function bad(): void {}",
            "Attribute \"AllowDynamicProperties\" cannot target function (allowed targets: class)",
        ),
    ] {
        let error = check_source(source).expect_err("invalid Core attribute target must fail");
        assert_eq!(error, expected, "source: {source}");
    }
}

/// Target errors report the offending attribute's own position for every declaration kind.
#[test]
fn test_core_symbols_attribute_target_spans() {
    let cases = [
        ("<?php\n#[SensitiveParameter]\nclass Bad {}", (2, 3), "class"),
        ("<?php\n#[SensitiveParameter]\nfunction bad(): void {}", (2, 3), "function"),
        ("<?php\nclass Bad {\n    #[SensitiveParameter]\n    public function bad(): void {}\n}", (3, 7), "method"),
        ("<?php\nclass Bad {\n    #[SensitiveParameter]\n    public int $value = 0;\n}", (3, 7), "property"),
        ("<?php\nclass Bad {\n    #[SensitiveParameter]\n    public const VALUE = 1;\n}", (3, 7), "class constant"),
        ("<?php\nenum Bad {\n    #[SensitiveParameter]\n    case One;\n}", (3, 7), "class constant"),
        ("<?php\nfunction bad(\n    int $first,\n    #[AllowDynamicProperties] int $second,\n): void {}", (4, 7), "parameter"),
        (
            "<?php\nclass Bad {\n    public function bad(\n        #[AllowDynamicProperties] $value,\n    ): void {}\n}",
            (4, 11),
            "parameter",
        ),
    ];
    for (source, position, target) in cases {
        let error = check_source_full(source).err().expect("invalid Core attribute target must fail");
        assert_eq!((error.span.line, error.span.col), position, "source: {source}");
        assert!(error.message.contains(&format!("cannot target {target} (")), "{}", error.message);
    }
    let error = check_source_full("<?php\n#[Attribute]\n#[Deprecated,\n  SensitiveParameter]\nfinal class Bad {}")
        .err()
        .expect("the second group's second attribute must fail");
    assert_eq!((error.span.line, error.span.col), (4, 3));
    assert!(error.message.starts_with("Attribute \"SensitiveParameter\""), "{}", error.message);
}

/// Unused interfaces, traits, and enum members are validated like PHP compiling each declaration.
#[test]
fn test_core_symbols_attribute_targets_cover_every_class_like() {
    for (source, expected) in [
        ("<?php #[Override] interface Bad {}", "Attribute \"Override\" cannot target class (allowed targets: method, property)"),
        ("<?php #[Override] trait Bad {}", "Attribute \"Override\" cannot target class (allowed targets: method, property)"),
        ("<?php interface Bad { #[Override] const VALUE = 1; }", "Attribute \"Override\" cannot target class constant (allowed targets: method, property)"),
        ("<?php trait Bad { #[AllowDynamicProperties] public int $value = 1; }", "Attribute \"AllowDynamicProperties\" cannot target property (allowed targets: class)"),
        ("<?php enum Bad { #[AllowDynamicProperties] public function bad(): void {} }", "Attribute \"AllowDynamicProperties\" cannot target method (allowed targets: class)"),
    ] {
        assert_eq!(check_source(source).unwrap_err(), expected, "source: {source}");
    }
}

/// Attribute errors follow PHP's compile order, ahead of inheritance errors and later declarations.
#[test]
fn test_core_symbols_attribute_error_precedence() {
    for (source, expected) in [
        // A repeat anywhere in the list wins over a later attribute's target.
        (
            "<?php class Bad { #[Override, AllowDynamicProperties, Override] public function run(): void {} }",
            "Attribute \"Override\" must not be repeated",
        ),
        // A target error wins over a repeat of the same attribute.
        (
            "<?php #[AllowDynamicProperties, AllowDynamicProperties] function bad(): void {}",
            "Attribute \"AllowDynamicProperties\" cannot target function (allowed targets: class)",
        ),
        // Members are reported in source order, not grouped by kind.
        (
            "<?php class Bad { #[Override] public const FIRST = 1; #[AllowDynamicProperties] public int $second = 2; }",
            "Attribute \"Override\" cannot target class constant (allowed targets: method, property)",
        ),
        // An invalid target is reported before the link-time Override check of a sibling method.
        (
            "<?php class Bad { #[Override] public function run(): void {} #[SensitiveParameter] public int $value = 0; }",
            "Attribute \"SensitiveParameter\" cannot target property (allowed targets: parameter)",
        ),
        // Repetition on a promoted property's copy is not lost by the owner split.
        (
            "<?php class P { public int $x = 0; } class Bad extends P { public function __construct(#[Override, Override] public int $x) {} }",
            "Attribute \"Override\" must not be repeated",
        ),
    ] {
        assert_eq!(check_source(source).unwrap_err(), expected, "source: {source}");
    }
}

/// Type-checks `source` under `php_version`, restoring the default profile for this thread.
fn check_source_for_php_version(source: &str, php_version: PhpVersion) -> Result<(), String> {
    elephc::codegen::set_compile_profile(php_version, false);
    let result = check_source(source);
    elephc::codegen::set_compile_profile(PhpVersion::default(), false);
    result
}

/// Issue #2202: Core targets follow `--php-version`, including their allowed-target wording.
#[test]
fn test_core_symbols_attribute_targets_follow_php_version() {
    let inherited = "<?php class P { public int $x = 0; } class C extends P { #[Override] public int $x = 1; }";
    let override_property = "Attribute \"Override\" cannot target property (allowed targets: method)";
    for (source, php_version, expected) in [
        ("<?php class C { #[Override] public int $x = 1; }", PhpVersion::Php84, Some(override_property)),
        (inherited, PhpVersion::Php83, Some(override_property)),
        (inherited, PhpVersion::Php84, Some(override_property)),
        (inherited, PhpVersion::Php85, None),
        // Both owners keep an attribute matching neither, and PHP reports the parameter.
        (
            "<?php class P { public int $x = 0; } class C extends P { public function __construct(#[Override] public int $x) {} }",
            PhpVersion::Php84,
            Some("Attribute \"Override\" cannot target parameter (allowed targets: method)"),
        ),
        // Deprecated gained classes and global constants in 8.5.
        ("<?php #[Deprecated] trait T {}", PhpVersion::Php84, Some("Attribute \"Deprecated\" cannot target class (allowed targets: function, method, class constant)")),
        ("<?php #[Deprecated] trait T {}", PhpVersion::Php85, None),
        // A name the profile does not predefine is an ordinary user attribute.
        ("<?php #[Deprecated] class C {}", PhpVersion::Php83, None),
        ("<?php #[NoDiscard] class C {}", PhpVersion::Php84, None),
        ("<?php #[NoDiscard] #[NoDiscard] function f(): int { return 1; }", PhpVersion::Php84, None),
        ("<?php #[NoDiscard] function f(): void {}", PhpVersion::Php84, None),
        ("<?php #[NoDiscard] function f(): void {}", PhpVersion::Php85, Some("NoDiscard cannot apply to a void or never function")),
        ("<?php #[Override] class C {}", PhpVersion::Php82, None),
        ("<?php #[Override] class C {}", PhpVersion::Php83, Some("Attribute \"Override\" cannot target class (allowed targets: method)")),
        ("<?php #[SensitiveParameter] class C {}", PhpVersion::Php81, None),
        // The deferral marker itself only exists from 8.5.
        ("<?php #[DelayedTargetValidation, SensitiveParameter] class C {}", PhpVersion::Php84, Some("Attribute \"SensitiveParameter\" cannot target class (allowed targets: parameter)")),
        ("<?php #[DelayedTargetValidation, SensitiveParameter] class C {}", PhpVersion::Php85, None),
    ] {
        assert_eq!(check_source_for_php_version(source, php_version).err().as_deref(), expected, "{php_version}: {source}");
    }
}

/// NoDiscard is not a Core marker below 8.5, so its discard warning follows the profile.
#[test]
fn test_core_symbols_no_discard_warning_follows_php_version() {
    let source = "<?php #[NoDiscard] function result(): int { return 7; } result();";
    let warns = |php_version| {
        elephc::codegen::set_compile_profile(php_version, false);
        let result = check_source_full(source).expect("expected source to type-check");
        elephc::codegen::set_compile_profile(PhpVersion::default(), false);
        result.warnings.iter().any(|warning| warning.message.contains("The return value of result()"))
    };
    assert!(!warns(PhpVersion::Php84));
    assert!(warns(PhpVersion::Php85));
}

/// Core validation preserves promoted-owner selection, repeatability, and return contracts.
#[test]
fn test_core_symbols_attribute_target_errors() {
    // A child's plain parameter named like a parent's promoted property is not promoted.
    expect_error(
        "<?php class P { public function __construct(public int $x) {} }
class C extends P { public function __construct(#[Override] int $x) { parent::__construct($x); } }",
        "Attribute \"Override\" cannot target parameter (allowed targets: method, property)",
    );
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
