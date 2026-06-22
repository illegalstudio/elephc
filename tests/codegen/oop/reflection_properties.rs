//! Purpose:
//! End-to-end codegen tests for ReflectionProperty value accessors on supported
//! object-property storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers explicit object arguments for public instance properties.
//! - Covers static properties where PHP permits no object argument.
//! - Visibility-bypassing reflection access remains a separate surface.

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
