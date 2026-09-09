//! Purpose:
//! Verifies array_map over PHP array parameters whose packed or hash storage is selected at runtime.
//!
//! Called from:
//! - The runtime GC codegen suite on every executable target.
//!
//! Key details:
//! - Declared array parameters prevent literal specialization from bypassing boxed dispatch.
//! - Snapshots preserve keys and values across callback mutations and source-owner release.

use crate::support::*;

/// A callback extracted from a returned PHP array retains captures after the array owner disappears.
#[test]
fn test_core_php_array_map_extracted_callback_retains_captures() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function returnedMapCallbacks(string $prefix): array {
    $callback = function(string $name) use ($prefix): string { return $prefix . $name; };
    return [$callback];
}
$total = 0;
for ($i = 0; $i < 20; $i++) {
    $callbacks = returnedMapCallbacks("old");
    $callback = $callbacks[0];
    unset($callbacks);
    $mapped = array_map($callback, ["Ada"]);
    $total += strlen($mapped[0]);
    unset($mapped, $callback);
}
echo $total;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "120", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Boxed string, method-pair, invokable and null callbacks preserve keys and independent array owners.
#[test]
fn test_core_php_array_map_boxed_callback_shapes_and_null_identity() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function namedBoxedMap(string $name): string { return "name:" . $name; }
class BoxedMapReceiver {
    public function render(string $name): string { return "method:" . $name; }
    public function __invoke(string $name): string { return "invoke:" . $name; }
}
function mapBoxedCallback(mixed $callback, array $items): array {
    return array_map($callback, $items);
}
$source = ["key" => "Ada"];
$receiver = new BoxedMapReceiver();
$named = mapBoxedCallback("namedBoxedMap", $source);
$method = mapBoxedCallback([$receiver, "render"], $source);
$invoked = mapBoxedCallback($receiver, $source);
$identity = mapBoxedCallback(null, $source);
$direct = array_map(null, ["direct"]);
echo $named["key"], "|", $method["key"], "|", $invoked["key"], "|", $direct[0], "|";
$identity["key"] = "changed";
echo $source["key"], ":", $identity["key"];
unset($source, $receiver, $named, $method, $invoked, $identity, $direct);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "name:Ada|method:Ada|invoke:Ada|direct|Ada:changed", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Invalid boxed callbacks throw catchable TypeErrors before reading slots or invoking the map loop.
#[test]
fn test_core_php_array_map_invalid_boxed_callbacks_are_catchable() {
    let out = compile_and_run(r#"<?php
class InvalidBoxedMapReceiver {
    public function render(string $value): string { return $value; }
}
function rejectBoxedMapCallback(mixed $callback): void {
    try { $mapped = array_map($callback, ["value"]); echo "missed|"; }
    catch (TypeError $error) { echo "invalid|"; unset($error); }
}
rejectBoxedMapCallback(42);
rejectBoxedMapCallback("missingBoxedMapFunction");
rejectBoxedMapCallback([]);
rejectBoxedMapCallback([1, 2]);
rejectBoxedMapCallback([new InvalidBoxedMapReceiver(), "missing"]);
rejectBoxedMapCallback(new InvalidBoxedMapReceiver());
echo "done";
"#);
    assert_eq!(out, "invalid|invalid|invalid|invalid|invalid|invalid|done");
}

/// Throwing boxed callbacks retire their temporary descriptor lease before the captured object dies.
#[test]
fn test_core_php_array_map_throw_releases_boxed_callback_lease() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingBoxedMapCapture {
    public int $value = 7;
    public function __destruct() { echo "dropped|"; }
}
function invokeThrowingBoxedMap(mixed $callback, array $items): void {
    try { $mapped = array_map($callback, $items); }
    catch (RuntimeException $error) { echo "caught|"; unset($error); }
}
$object = new ThrowingBoxedMapCapture();
$callback = function(mixed $value) use ($object): mixed {
    echo $object->value, ":";
    throw new RuntimeException("map");
};
invokeThrowingBoxedMap($callback, ["key" => 1]);
unset($callback, $object);
echo "done";
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "7:caught|dropped|done", "{}", out.stderr);
}

/// Boxed mapping accepts packed, sparse, associative, and empty arrays with descriptor callbacks.
#[test]
fn test_core_php_array_map_layouts_and_keys() {
    let source = r#"<?php
function mapPhpValues(array $items, callable $callback): array { return array_map($callback, $items); }
function labelPhpValue(string $value): string { return "[" . $value . "]"; }
function printMappedPhpValues(array $items): void {
    $mapped = mapPhpValues($items, labelPhpValue(...));
    echo implode(",", array_keys($mapped)), ":", implode(",", $mapped), "|";
}
printMappedPhpValues(["a", "b"]);
printMappedPhpValues([7 => "a", "name" => "b", -2 => "c"]);
printMappedPhpValues([]);
"#;
    assert_eq!(compile_and_run(source), "0,1:[a],[b]|7,name,-2:[a],[b],[c]|:|");
}

/// Mapping a boxed array snapshots its payload before a by-reference callback rewrites the source.
#[test]
fn test_core_php_array_map_snapshots_callback_mutations() {
    let source = r#"<?php
function mutateMappedPhpValues(array &$items): void {
    $mapped = array_map(function(mixed $value) use (&$items): mixed {
        $items["second"] = "changed";
        $items["added"] = "later";
        return $value;
    }, $items);
    echo implode(",", array_keys($mapped)), ":", implode(",", $mapped), "|";
    echo implode(",", array_keys($items)), ":", implode(",", $items);
}
$items = ["first" => "a", "second" => "b"];
mutateMappedPhpValues($items);
"#;
    assert_eq!(compile_and_run(source), "first,second:a,b|first,second,added:a,changed,later");
}

/// Mapped Mixed results retain nested arrays and object identity after the source owners disappear.
#[test]
fn test_core_php_array_map_mixed_result_owners_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class PhpMappedObject { public int $value = 7; }
function keepPhpMapValue(mixed $value): mixed { return $value; }
function keepPhpMapOwners(array $items): array { return array_map(keepPhpMapValue(...), $items); }
$object = new PhpMappedObject();
$items = ["object" => $object, 5 => ["name" => "kept"], "number" => 2];
$mapped = keepPhpMapOwners($items);
unset($items, $object);
echo $mapped["object"]->value, ":", $mapped[5]["name"], ":", $mapped["number"];
unset($mapped);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:kept:2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
