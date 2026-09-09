//! Purpose:
//! Verifies caller reads after declared PHP array reference parameters replace array layouts.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Named and callable calls must leave the same storage contract as direct calls.
//! - Conditional conversions must preserve both taken and skipped paths and COW aliases.

use crate::support::*;

/// Reference iteration changes the property and its reference alias, never an earlier value copy.
#[test]
fn test_core_native_php_array_reference_property_iteration_preserves_value_copy() {
    let source = r#"<?php
class BoxedIterationProperty {
    public array $items = [1, "two"];
}
$owner = new BoxedIterationProperty();
$reference = &$owner->items;
$snapshot = $owner->items;
foreach ($owner->items as &$value) { $value = "v" . $value; }
unset($value);
echo implode(",", $owner->items), "|", implode(",", $reference), "|", implode(",", $snapshot);
"#;
    assert_eq!(compile_and_run(source), "v1,vtwo|v1,vtwo|1,two");
}

/// Every statically resolved call surface updates the caller's array key and value storage types.
#[test]
fn test_core_native_php_array_reference_output_call_surfaces() {
    let source = r#"<?php
function writeArrayKey(array &$items): void { $items["added"] = 7; }
class ReferenceArrayWriter {
    public function write(array &$items): void { $items["added"] = 8; }
    public static function writeStatic(array &$items): void { $items["added"] = 9; }
}
$named = [1];
writeArrayKey(items: $named);
echo implode(",", array_keys($named)), ":", implode(",", $named), "|";
$callback = writeArrayKey(...);
$callable = [2];
$callback($callable);
echo implode(",", array_keys($callable)), ":", implode(",", $callable), "|";
$writer = new ReferenceArrayWriter();
$method = [3];
$writer->write(items: $method);
echo implode(",", array_keys($method)), ":", implode(",", $method), "|";
$static = [4];
ReferenceArrayWriter::writeStatic($static);
echo implode(",", array_keys($static)), ":", implode(",", $static), "|";
$closure = function(array &$items): void { $items["added"] = 10; };
$captured = [5];
$closure($captured);
echo implode(",", array_keys($captured)), ":", implode(",", $captured);
"#;
    assert_eq!(compile_and_run(source), "0,added:1,7|0,added:2,7|0,added:3,8|0,added:4,9|0,added:5,10");
}

/// Both arms of short-circuit calls keep packed and keyed locals valid after reference boxing.
#[test]
fn test_core_native_php_array_reference_output_conditional_storage() {
    let source = r#"<?php
function conditionallyWriteArray(array &$items): bool { $items["added"] = 7; return true; }
function checkConditionalArray(bool $write): void {
    $packed = [1];
    $hash = ["old" => 2];
    $copy = $packed;
    $write && conditionallyWriteArray($packed);
    $write && conditionallyWriteArray($hash);
    echo implode(",", array_keys($packed)), ":", implode(",", $packed), "|";
    echo implode(",", array_keys($hash)), ":", implode(",", $hash), "|";
    echo implode(",", array_keys($copy)), ":", implode(",", $copy), ";";
}
checkConditionalArray($argc > 0);
checkConditionalArray($argc < 0);
"#;
    assert_eq!(compile_and_run(source), "0,added:1,7|old,added:2,7|0:1;0:1|old:2|0:1;");
}
