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
