//! Purpose:
//! End-to-end codegen tests for ReflectionProperty value accessors on supported
//! object-property storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers explicit object arguments for public and non-public instance properties.
//! - Covers runtime-held public instance property reflectors.
//! - Covers static properties where PHP permits omitted or ignored object args.
//! - Covers Reflection visibility bypass for instance and inline static properties.

use super::*;

/// Verifies `ReflectionProperty::getValue()` and `setValue()` read and write
/// public instance properties for inline reflectors with explicit object args,
/// including reflectors returned by `ReflectionClass::getProperty()`.
#[test]
fn test_reflection_property_value_accessors_for_public_instance_properties() {
    let out = compile_and_run(
        r#"<?php
class ReflectValueAccessTarget {
    public int $count = 1;
    public string $label = "old";
}

$target = new ReflectValueAccessTarget();
echo (new ReflectionProperty(ReflectValueAccessTarget::class, "count"))->getValue($target);
(new ReflectionProperty(ReflectValueAccessTarget::class, "count"))->setValue($target, 7);
echo ":" . $target->count;
echo ":" . (new ReflectionProperty(ReflectValueAccessTarget::class, "label"))->getValue($target);
(new ReflectionProperty(ReflectValueAccessTarget::class, "label"))->setValue($target, "new");
echo ":" . $target->label;
echo ":" . (new ReflectionClass(ReflectValueAccessTarget::class))->getProperty("count")->getValue($target);
(new ReflectionClass(ReflectValueAccessTarget::class))->getProperty("count")->setValue($target, 11);
echo ":" . $target->count;
echo ":" . (new ReflectionProperty(ReflectValueAccessTarget::class, "count"))->getValue(object: $target);
(new ReflectionClass(ReflectValueAccessTarget::class))->getProperty("count")->setValue(value: 13, object: $target);
echo ":" . $target->count;
"#,
    );
    assert_eq!(out, "1:7:old:new:7:11:11:13");
}

/// Verifies `ReflectionProperty::getValue()` and `setValue()` read and write
/// public static properties for inline reflectors.
#[test]
fn test_reflection_property_value_accessors_for_public_static_properties() {
    let out = compile_and_run(
        r#"<?php
class ReflectStaticValueAccessTarget {
    public static int $count = 2;
    public static string $label = "old";
}

echo (new ReflectionProperty(ReflectStaticValueAccessTarget::class, "count"))->getValue();
(new ReflectionProperty(ReflectStaticValueAccessTarget::class, "count"))->setValue(null, 17);
echo ":" . ReflectStaticValueAccessTarget::$count;
ReflectStaticValueAccessTarget::$count = 19;
echo ":" . (new ReflectionProperty(ReflectStaticValueAccessTarget::class, "count"))->getValue(object: null);
echo ":" . (new ReflectionClass(ReflectStaticValueAccessTarget::class))->getProperty("label")->getValue(null);
(new ReflectionClass(ReflectStaticValueAccessTarget::class))->getProperty("label")->setValue(null, "new");
echo ":" . ReflectStaticValueAccessTarget::$label;
(new ReflectionClass(ReflectStaticValueAccessTarget::class))->getProperty("count")->setValue(object: null, value: 23);
echo ":" . ReflectStaticValueAccessTarget::$count;
"#,
    );
    assert_eq!(out, "2:17:19:old:new:23");
}

/// Verifies `ReflectionProperty::isInitialized()` observes typed instance
/// property initialization without reading the property value.
#[test]
fn test_reflection_property_is_initialized_for_instance_properties() {
    let out = compile_and_run(
        r#"<?php
class ReflectInitializedInstanceTarget {
    public int $typed;
    public ?string $nullable = null;
    public $implicit;
    private int $hidden;

    public function __construct() {
        $this->hidden = 7;
    }
}

$target = new ReflectInitializedInstanceTarget();
$typed = new ReflectionProperty(ReflectInitializedInstanceTarget::class, "typed");
echo $typed->isInitialized($target) ? "bad" : "uninit";
$target->typed = 3;
echo ":" . ($typed->isInitialized(object: $target) ? "typed" : "bad");
echo ":" . ((new ReflectionProperty(ReflectInitializedInstanceTarget::class, "nullable"))->isInitialized($target) ? "nullable" : "bad");
echo ":" . ((new ReflectionClass(ReflectInitializedInstanceTarget::class))->getProperty("implicit")->isInitialized($target) ? "implicit" : "bad");
echo ":" . ((new ReflectionProperty(ReflectInitializedInstanceTarget::class, "hidden"))->isInitialized($target) ? "hidden" : "bad");
"#,
    );
    assert_eq!(out, "uninit:typed:nullable:implicit:hidden");
}

/// Verifies `ReflectionProperty::isInitialized()` observes static-property
/// initialization while bypassing property visibility.
#[test]
fn test_reflection_property_is_initialized_for_static_properties() {
    let out = compile_and_run(
        r#"<?php
class ReflectInitializedStaticTarget {
    public static int $typed;
    public static ?string $nullable = null;
    private static int $hidden;

    public static function initHidden(): void {
        self::$hidden = 7;
    }
}

$typed = new ReflectionProperty(ReflectInitializedStaticTarget::class, "typed");
echo $typed->isInitialized() ? "bad" : "uninit";
ReflectInitializedStaticTarget::$typed = 3;
echo ":" . ($typed->isInitialized(object: null) ? "typed" : "bad");
echo ":" . ((new ReflectionProperty(ReflectInitializedStaticTarget::class, "nullable"))->isInitialized() ? "nullable" : "bad");
$hidden = (new ReflectionClass(ReflectInitializedStaticTarget::class))->getProperty("hidden");
echo ":" . ($hidden->isInitialized() ? "bad" : "hidden-uninit");
ReflectInitializedStaticTarget::initHidden();
echo ":" . ($hidden->isInitialized() ? "hidden" : "bad");
"#,
    );
    assert_eq!(out, "uninit:typed:nullable:hidden-uninit:hidden");
}

/// Verifies ReflectionProperty static value access bypasses visibility for
/// private and protected properties when the reflected target is statically known.
#[test]
fn test_reflection_property_value_accessors_bypass_static_visibility() {
    let out = compile_and_run(
        r#"<?php
class ReflectHiddenStaticValueAccessTarget {
    private static int $count = 4;
    protected static string $label = "old";

    public static function count(): int { return self::$count; }
    public static function label(): string { return self::$label; }
}

echo (new ReflectionProperty(ReflectHiddenStaticValueAccessTarget::class, "count"))->getValue();
(new ReflectionProperty(ReflectHiddenStaticValueAccessTarget::class, "count"))->setValue(null, 8);
echo ":" . ReflectHiddenStaticValueAccessTarget::count();

$label = (new ReflectionClass(ReflectHiddenStaticValueAccessTarget::class))->getProperty("label");
echo ":" . $label->getValue(null);
$label->setValue(object: null, value: "new");
echo ":" . ReflectHiddenStaticValueAccessTarget::label();
"#,
    );
    assert_eq!(out, "4:8:old:new");
}

/// Verifies static ReflectionProperty objects selected from `getProperties()`
/// with known indexes can read and write their reflected static storage.
#[test]
fn test_reflection_property_value_accessors_for_indexed_static_property_lists() {
    let out = compile_and_run(
        r#"<?php
class ReflectListedStaticValueAccessTarget {
    private static int $count = 4;
    protected static string $label = "old";

    public static function count(): int { return self::$count; }
    public static function label(): string { return self::$label; }
}

$count = (new ReflectionClass(ReflectListedStaticValueAccessTarget::class))->getProperties()[0];
echo $count->getName() . ":" . $count->getValue();
$count->setValue(null, 8);
echo ":" . ReflectListedStaticValueAccessTarget::count();

$ref = new ReflectionClass(ReflectListedStaticValueAccessTarget::class);
$label = $ref->getProperties()[1];
echo ":" . $label->getName() . ":" . $label->getValue(null);
$label->setValue(null, "new");
echo ":" . ReflectListedStaticValueAccessTarget::label();
"#,
    );
    assert_eq!(out, "count:4:8:label:old:new");
}

/// Verifies `ReflectionClass::getProperties()` applies modifier filters and
/// that filtered static property entries retain direct value accessor support.
#[test]
fn test_reflection_property_value_accessors_for_filtered_static_property_lists() {
    let out = compile_and_run(
        r#"<?php
class ReflectFilteredStaticValueAccessTarget {
    public int $instance = 1;
    private static int $count = 4;
    protected static string $label = "old";

    public static function count(): int { return self::$count; }
    public static function label(): string { return self::$label; }
    public function touch(): void {}
    private function secret(): void {}
    protected static function helper(): void {}
}

$ref = new ReflectionClass(ReflectFilteredStaticValueAccessTarget::class);
$static = $ref->getProperties(ReflectionProperty::IS_STATIC);
echo count($static) . ":" . $static[0]->getName();

$count = $ref->getProperties(ReflectionProperty::IS_STATIC)[0];
echo ":" . $count->getValue();
$count->setValue(null, 8);
echo ":" . ReflectFilteredStaticValueAccessTarget::count();

$label = $ref->getProperties(filter: ReflectionProperty::IS_PROTECTED)[0];
echo ":" . $label->getName() . ":" . $label->getValue(null);
$label->setValue(null, "new");
echo ":" . ReflectFilteredStaticValueAccessTarget::label();

$public = $ref->getProperties(...["filter" => ReflectionProperty::IS_PUBLIC]);
$none = $ref->getProperties(0);
echo ":" . count($public) . ":" . $public[0]->getName() . ":" . count($none);

$staticMethods = $ref->getMethods(ReflectionMethod::IS_STATIC);
$privateMethods = $ref->getMethods(filter: ReflectionMethod::IS_PRIVATE);
$noMethods = $ref->getMethods(0);
echo ":" . count($staticMethods) . ":" . count($privateMethods) . ":" . count($noMethods);
"#,
    );
    assert_eq!(out, "2:count:4:8:label:old:new:1:instance:0:3:1:0");
}

/// Verifies runtime-held `ReflectionClass` objects apply dynamic member filters
/// without degrading returned reflector elements to unusable mixed payloads.
#[test]
fn test_reflection_class_runtime_member_filters_return_usable_reflectors() {
    let out = compile_and_run(
        r#"<?php
class ReflectRuntimeFilteredMemberTarget {
    public int $instance = 1;
    private static int $count = 4;
    protected static string $label = "old";

    public function touch(): void {}
    private function secret(): void {}
    protected static function helper(): void {}
}

function inspect_members(ReflectionClass $ref, int $propertyFilter, int $methodFilter): void {
    $props = $ref->getProperties($propertyFilter);
    echo count($props) . ":" . $props[0]->getName() . ":" . $props[1]->getName();

    $methods = $ref->getMethods($methodFilter);
    echo ":" . count($methods) . ":" . $methods[0]->getName() . ":" . $methods[0]->isPrivate();
}

inspect_members(
    new ReflectionClass(ReflectRuntimeFilteredMemberTarget::class),
    ReflectionProperty::IS_STATIC,
    ReflectionMethod::IS_PRIVATE
);
"#,
    );
    assert_eq!(out, "2:count:label:1:secret:1");
}

/// Verifies ReflectionClass static property value helpers bypass visibility and
/// operate on the same live static storage as direct class methods.
#[test]
fn test_reflection_class_static_value_accessors_bypass_visibility() {
    let out = compile_and_run(
        r#"<?php
class ReflectClassHiddenStaticValueTarget {
    private static int $count = 5;
    protected static string $label = "old";

    public static function count(): int { return self::$count; }
    public static function label(): string { return self::$label; }
}

$ref = new ReflectionClass(ReflectClassHiddenStaticValueTarget::class);
echo $ref->getStaticPropertyValue("count");
$ref->setStaticPropertyValue("count", 9);
echo ":" . ReflectClassHiddenStaticValueTarget::count();
echo ":" . $ref->getStaticPropertyValue("label");
$ref->setStaticPropertyValue(name: "label", value: "new");
echo ":" . ReflectClassHiddenStaticValueTarget::label();

$props = $ref->getStaticProperties();
echo ":" . $props["count"];
echo ":" . $props["label"];
"#,
    );
    assert_eq!(out, "5:9:old:new:9:new");
}

/// Verifies runtime-held `ReflectionClass` objects expose materialized AOT
/// static-property values and omit uninitialized typed static properties.
#[test]
fn test_reflection_class_runtime_static_properties_materialize_aot_values() {
    let out = compile_and_run(
        r#"<?php
class ReflectRuntimeStaticPropertiesTarget {
    public static int $count = 2;
    private static string $label = "old";
    public static ?string $nullable = null;
    public static int $unset;

    public static function rename(string $value): void {
        self::$label = $value;
    }
}

function inspect_static_props(ReflectionClass $ref): void {
    $props = $ref->getStaticProperties();
    echo count($props);
    echo ":" . $props["count"];
    echo ":" . $props["label"];
    echo ":" . (array_key_exists("nullable", $props) && $props["nullable"] === null ? "null" : "bad");
    echo ":" . ($props["unset"] ?? "missing");
    echo ":" . $ref->getStaticPropertyValue("count", "fallback");
    echo ":" . ($ref->getStaticPropertyValue("nullable", "fallback") === null ? "null" : "bad");
    echo ":" . $ref->getStaticPropertyValue("missing", "fallback");
}

ReflectRuntimeStaticPropertiesTarget::$count = 5;
ReflectRuntimeStaticPropertiesTarget::rename("new");
inspect_static_props(new ReflectionClass(ReflectRuntimeStaticPropertiesTarget::class));
"#,
    );
    assert_eq!(out, "3:5:new:null:missing:5:null:fallback");
}

/// Verifies runtime-held `ReflectionProperty` objects can read and write public
/// instance properties through their retained property names.
#[test]
fn test_reflection_property_value_accessors_for_runtime_instance_reflectors() {
    let out = compile_and_run(
        r#"<?php
class ReflectRuntimeValueAccessTarget {
    public int $count = 4;
    public string $label = "old";
}

$target = new ReflectRuntimeValueAccessTarget();
$count = new ReflectionProperty(ReflectRuntimeValueAccessTarget::class, "count");
echo $count->getValue($target);
$count->setValue($target, 8);
echo ":" . $target->count;

$listed = (new ReflectionClass(ReflectRuntimeValueAccessTarget::class))->getProperties()[1];
echo ":" . $listed->getName();
echo ":" . $listed->getValue($target);
$listed->setValue($target, "new");
echo ":" . $target->label;
"#,
    );
    assert_eq!(out, "4:8:label:old:new");
}

/// Verifies ReflectionProperty value access bypasses visibility for private
/// and protected instance properties, matching PHP's Reflection behavior.
#[test]
fn test_reflection_property_value_accessors_bypass_instance_visibility() {
    let out = compile_and_run(
        r#"<?php
class ReflectHiddenValueAccessTarget {
    private int $count = 4;
    protected string $label = "old";
}

$target = new ReflectHiddenValueAccessTarget();
$count = new ReflectionProperty(ReflectHiddenValueAccessTarget::class, "count");
echo $count->getValue($target);
$count->setValue($target, 8);
echo ":" . $count->getValue($target);

$label = (new ReflectionClass(ReflectHiddenValueAccessTarget::class))->getProperty("label");
echo ":" . $label->getValue($target);
$label->setValue($target, "new");
echo ":" . $label->getValue($target);
"#,
    );
    assert_eq!(out, "4:8:old:new");
}

/// Verifies `ReflectionProperty::setAccessible()` is a no-op for AOT reflectors.
#[test]
fn test_reflection_property_set_accessible_is_noop_for_aot_properties() {
    let out = compile_and_run(
        r#"<?php
class ReflectPropertyAccessTarget {
    private int $count = 4;
    protected string $label = "old";
}

$target = new ReflectPropertyAccessTarget();
$count = new ReflectionProperty(ReflectPropertyAccessTarget::class, "count");
echo is_null($count->setAccessible(false)) ? "P" : "p";
echo ":" . $count->getValue($target);
$count->setValue($target, 9);
echo ":" . $count->getValue($target);
echo ":";
$label = (new ReflectionClass(ReflectPropertyAccessTarget::class))->getProperty("label");
echo is_null($label->setAccessible(accessible: true)) ? "L" : "l";
echo ":" . $label->getValue($target);
"#,
    );
    assert_eq!(out, "P:4:9:L:old");
}

/// Verifies AOT `ReflectionProperty::__toString()` formats retained generated
/// property metadata.
#[test]
fn test_reflection_property_to_string_formats_aot_metadata() {
    let out = compile_and_run(
        r#"<?php
class ReflectPropertyStringTarget {
    public int $id = 7;
    protected static string $label = "ok";
    private $implicit;
    public int|string $union;
}

echo (new ReflectionProperty(ReflectPropertyStringTarget::class, "id"))->__toString();
echo "|";
echo (new ReflectionProperty(ReflectPropertyStringTarget::class, "label"))->__toString();
echo "|";
echo (new ReflectionProperty(ReflectPropertyStringTarget::class, "implicit"))->__toString();
echo "|";
echo (new ReflectionProperty(ReflectPropertyStringTarget::class, "union"))->__toString();
echo "|";
echo (new ReflectionClass(ReflectPropertyStringTarget::class))->getProperty("label")->__toString();
"#,
    );
    assert_eq!(
        out,
        "Property [ public int $id = 7 ]|Property [ protected static string $label = 'ok' ]|Property [ private $implicit = NULL ]|Property [ public int|string $union ]|Property [ protected static string $label = 'ok' ]"
    );
}
