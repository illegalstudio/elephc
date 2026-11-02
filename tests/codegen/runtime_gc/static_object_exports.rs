//! Purpose:
//! Verifies concrete object stores into nullable function-static storage.
//!
//! Called from:
//! - The executable codegen test binary's runtime ownership tests.
//!
//! Key details:
//! - The persistent Mixed slot owns a box and a reference independent of its source.

use crate::support::compile_and_run_with_heap_debug;

/// Keeps nullable static object assignments boxed and retained across caller cleanup.
#[test]
fn test_static_nullable_object_store_owns_boxed_payload() {
    let output = compile_and_run_with_heap_debug(r#"<?php
final class StaticObjectValue { public int $value = 42; }
function object_cache(?StaticObjectValue $object, bool $set): ?StaticObjectValue {
    static $cached = null;
    if ($set) { $cached = $object; }
    return $cached;
}
function created_cache(bool $clear): ?StaticObjectValue {
    static $value = null;
    if ($clear) { $value = null; return null; }
    if ($value === null) { $value = new StaticObjectValue(); }
    return $value;
}
$object = new StaticObjectValue();
var_dump(object_cache($object, true) === $object);
unset($object);
echo object_cache(null, false)->value, "\n";
object_cache(null, true);
var_dump(object_cache(null, false));
echo created_cache(false)->value, "\n";
created_cache(true);
function scalar_cache(bool $set): mixed {
    static $value = null;
    if ($set) { $value = 42; }
    return $value;
}
var_dump(scalar_cache(false), scalar_cache(true), scalar_cache(false));
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "bool(true)\n42\nNULL\n42\nNULL\nint(42)\nint(42)\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Resolves global array constants before a namespaced class validates its typed metadata.
#[test]
fn test_namespaced_class_array_constant_defaults_are_typed_before_schema() {
    let output = compile_and_run_with_heap_debug(r#"<?php
namespace ConstantDefault;
const OPTIONS = ['nested' => ['value' => 'original']];
final class Container {
    public const array DATA = OPTIONS;
    public array $options = OPTIONS;
    public static function create(): self { return new self(); }
}
$a = Container::create();
$a->options['nested']['value'] = 'changed';
$b = Container::create();
echo $b->options['nested']['value'], "\n";
echo Container::DATA['nested']['value'], "\n";
unset($a, $b);
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "original\noriginal\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
