//! Purpose:
//! End-to-end codegen tests for ReflectionProperty value accessors on supported
//! object-property storage.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers explicit object arguments for public instance properties.
//! - Static properties and visibility-bypassing reflection access remain separate surfaces.

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
