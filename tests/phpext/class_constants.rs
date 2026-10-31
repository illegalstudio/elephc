//! Purpose:
//! Compares hosted class-constant metadata and native constant reads with real PHP.
//!
//! Called from:
//! - The PHP SDK integration binary and SDK-independent rejection checks.
//!
//! Key details:
//! - Public Reflection data and private implementation reads share the same numeric contract.
//! - Unsupported widening fails before SDK discovery without changing function-only modules.

use super::*;

/// Matches PHP's materialized values for nullable, union and mixed typed class constants.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_classes_native_typed_float_constant_unions_match_php() {
    let project = Project::new();
    let sdk = project.sdk();
    let source = r#"<?php
interface FloatUnits { public const int BASE = 2; public const float SCALE = self::BASE; }
interface ChildFloatUnits extends FloatUnits { public const int BASE = 3; }
#[Export] final class FloatConstants {
    private const int BASE = 2;
    private const int INDEX = 0;
    private const float PRIVATE_COMPUTED = [self::BASE][self::INDEX];
    private const float|FloatConstants NOMINAL = 2;
    private const string STRING_COMPUTED = '2'[self::INDEX];
    private const array SOURCE = [9 => 4, 0 => 8, 9 => 2, 'x' => 6];
    private const float SPREAD_COMPUTED = [...self::SOURCE][0];
    private const float SPREAD_STRING = [...self::SOURCE]['x'];
    public const float EXACT = 2;
    public const ?float NULLABLE = 2;
    public const float|null UNION_NULL = 2;
    public const float|string UNION_STRING = 2;
    public const int|float UNION_INT = 2;
    public const float|int REVERSED_INT = 2;
    public const ?float NULL_VALUE = null;
    public const mixed MIXED_VALUE = 2;
    public const COPIED = self::EXACT;
    public const float COMPUTED = [self::BASE][self::INDEX];
    public const COMPUTED_ALIAS = self::PRIVATE_COMPUTED;
    public const array COMPUTED_NESTED = [self::PRIVATE_COMPUTED];
    public const NOMINAL_ALIAS = self::NOMINAL;
    public const float SPREAD = [...self::SOURCE][0];
    public const float INTERFACE_COPY = ChildFloatUnits::SCALE;
    public const array NESTED = [self::NULLABLE];
    public mixed $copied = self::UNION_STRING;
    public float $interfaceDefault = ChildFloatUnits::SCALE;
    public function exactValue(): mixed { return self::EXACT; }
    public function nullableValue(): mixed { return self::NULLABLE; }
    public function unionNullValue(): mixed { return self::UNION_NULL; }
    public function stringUnionValue(): mixed { return self::UNION_STRING; }
    public function numericUnionValue(): mixed { return self::UNION_INT; }
    public function reversedNumericValue(): mixed { return self::REVERSED_INT; }
    public function nullValue(): mixed { return self::NULL_VALUE; }
    public function mixedValue(): mixed { return self::MIXED_VALUE; }
    public function copiedValue(): mixed { return self::COPIED; }
    public function nestedValue(): mixed { return self::NESTED; }
    public function privateComputedValue(): mixed { return self::PRIVATE_COMPUTED; }
    public function nominalValue(): mixed { return self::NOMINAL; }
    public function stringValue(): mixed { return self::STRING_COMPUTED; }
    public function spreadValue(): mixed { return self::SPREAD_COMPUTED; }
    public function spreadStringValue(): mixed { return self::SPREAD_STRING; }
    public function interfaceValue(): mixed { return FloatUnits::SCALE; }
    public function childInterfaceValue(): mixed { return ChildFloatUnits::SCALE; }
    public function computedValue(): mixed { return self::COMPUTED; }
    public function computedAliasValue(): mixed { return self::COMPUTED_ALIAS; }
    public function computedNestedValue(): mixed { return self::COMPUTED_NESTED; }
    public function computedArray(): array { return [COMPUTED_COPY]; }
    public function computedClassArray(): array { return [self::COMPUTED_ALIAS]; }
    public function globalArray(): array { return [FLOAT_COPY]; }
    public function classArray(): array { return [self::COPIED]; }
    public static function nativeValues(): array {
        return [self::EXACT, self::NULLABLE, self::UNION_NULL, self::UNION_STRING,
            self::UNION_INT, self::REVERSED_INT, self::NULL_VALUE, self::MIXED_VALUE,
            self::COPIED, self::NESTED, FLOAT_COPY, self::COMPUTED, self::COMPUTED_ALIAS, self::COMPUTED_NESTED, COMPUTED_COPY];
    }
}
const FLOAT_COPY = FloatConstants::NULLABLE;
const COMPUTED_COPY = FloatConstants::COMPUTED_ALIAS;
"#;
    let module = project.compile_with_options("class_float_constants", source,
        &["--heap-size=524288", "--export-constant=FLOAT_COPY", "--export-constant=COMPUTED_COPY"]);
    let check = r#"
$values = [];
foreach ((new ReflectionClass(FloatConstants::class))->getReflectionConstants(ReflectionClassConstant::IS_PUBLIC) as $constant) {
    $values[] = [constant('FloatConstants::' . $constant->getName()), $constant->getValue(), (string)$constant->getType()];
}
$values[] = FLOAT_COPY;
$values[] = COMPUTED_COPY;
$instance = new FloatConstants();
$values[] = $instance->copied;
$values[] = $instance->interfaceDefault;
foreach (['exactValue', 'nullableValue', 'unionNullValue', 'stringUnionValue', 'numericUnionValue',
    'reversedNumericValue', 'nullValue', 'mixedValue', 'copiedValue', 'nestedValue', 'privateComputedValue', 'computedValue', 'computedAliasValue', 'computedNestedValue',
    'nominalValue', 'stringValue', 'spreadValue', 'spreadStringValue', 'interfaceValue', 'childInterfaceValue'] as $method) {
    $values[] = $instance->$method();
}
$values[] = FloatConstants::nativeValues();
$values[] = $instance->globalArray();
$values[] = $instance->classArray();
$values[] = $instance->computedArray();
$values[] = $instance->computedClassArray();
echo json_encode($values, JSON_PRESERVE_ZERO_FRACTION | JSON_THROW_ON_ERROR), "\n";
"#;
    let reference = project.0.join("reference.php");
    fs::write(&reference, format!("{source}\n{check}")).unwrap();
    let expected = Command::new(sdk.php_binary()).arg("-n").arg(&reference).output().unwrap();
    assert!(expected.status.success(), "{}", String::from_utf8_lossy(&expected.stderr));
    assert!(expected.stderr.is_empty(), "{}", String::from_utf8_lossy(&expected.stderr));
    let actual = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", check]).output().unwrap();
    assert!(actual.status.success(), "{}", String::from_utf8_lossy(&actual.stderr));
    assert!(actual.stderr.is_empty(), "{}", String::from_utf8_lossy(&actual.stderr));
    assert_eq!(String::from_utf8_lossy(&actual.stdout), String::from_utf8_lossy(&expected.stdout));
    project.assert_clean();
}

/// Rejects unresolved widening before SDK work while preserving declaration pruning in modules exporting functions.
#[test]
fn phpext_classes_float_constants_require_static_widening_before_sdk() {
    let project = Project::new();
    for (index, source) in [
        "<?php #[Export] final class OpaqueFloat { private const float VALUE = [2][0.5]; public function value(): mixed { return self::VALUE; } }",
        "<?php #[Export] final class OpaqueStringFloat { private const float VALUE = '2'[0] + 0; public function value(): mixed { return self::VALUE; } }",
        "<?php interface OpaqueUnits { public const float VALUE = [2][0.5]; } #[Export] final class OpaqueMeter { public function value(): mixed { return OpaqueUnits::VALUE; } }",
    ].iter().enumerate() {
        let path = project.0.join(format!("opaque_float_{index}.php"));
        fs::write(&path, source).unwrap();
        for terminal in ["--check", "--emit-ir"] {
            let output = Command::new(compiler_binary()).args(["--emit", "phpext", terminal]).arg(&path)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{source}: {terminal}");
            assert!(stderr.contains("requires a statically resolved value for integer-to-float coercion"),
                "{source}: {terminal}: {stderr}");
            assert!(!stderr.contains("query php-config"), "{source}: {terminal}: {stderr}");
        }
    }
    let path = project.0.join("unused_opaque_float.php");
    fs::write(&path, "<?php class UnusedOpaqueFloat { private const float VALUE = [2][0.5]; } #[Export] function plain_export(): int { return 42; }").unwrap();
    let output = Command::new(compiler_binary()).args(["--emit", "phpext", "--check"]).arg(&path)
        .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let path = project.0.join("unused_opaque_with_class.php");
    fs::write(&path, "<?php class AnotherUnusedOpaque { private const float VALUE = [2][0.5]; } interface AnotherUnusedUnits { public const float VALUE = [2][0.5]; } #[Export] final class PlainExportedClass {}").unwrap();
    let output = Command::new(compiler_binary()).args(["--emit", "phpext", "--check"]).arg(&path)
        .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

