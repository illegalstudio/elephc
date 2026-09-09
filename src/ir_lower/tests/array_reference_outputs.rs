//! Purpose:
//! Verifies caller storage and builtin result types after declared PHP array reference calls.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Validation and assembly emission cover every supported target without running a native binary.

use crate::codegen::platform::Target;
use std::path::Path;

/// Reference iteration detaches boxed property values before exposing a borrowed array source.
#[test]
fn php_array_property_reference_iteration_separates_cells_on_every_target() {
    use crate::ir::{Op, Ownership};

    let source = r#"<?php
class PropertyArrayReference {
    public array $packed = [1, 2];
    public array $hash = ["a" => 1, "b" => 2];
}
$owner = new PropertyArrayReference();
$packedRef = &$owner->packed;
$hashRef = &$owner->hash;
$packedCopy = $owner->packed;
$hashCopy = $owner->hash;
foreach ($owner->packed as &$value) { $value = $value * 2; }
unset($value);
foreach ($owner->hash as &$value) { $value = $value * 2; }
unset($value);
echo implode(",", $packedRef), implode(",", $packedCopy), implode(",", $hashRef), implode(",", $hashCopy);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let fetches = module.functions.iter().flat_map(|function| &function.instructions)
            .filter(|instruction| instruction.op == Op::PropGetForWrite).collect::<Vec<_>>();
        assert_eq!(fetches.len(), 2, "{name}");
        assert!(fetches.iter().all(|instruction| instruction.result_ownership == Ownership::Borrowed), "{name}");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_mixed_clone"), "{name}");
    }
}

/// Destructuring a boxed array keeps its source alive across stores on every supported ABI.
#[test]
fn php_array_list_unpack_lowers_on_every_target() {
    let source = r#"<?php
function readBoxedRow(array $items): string {
    [$items, $tail] = $items;
    return $items . ":" . $tail;
}
echo readBoxedRow([1 => "second", 0 => "first"]);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Positional, named, callable, method and conditional reference calls preserve keyed output types.
#[test]
fn php_array_reference_outputs_lower_on_every_target() {
    let source = r#"<?php
function addKey(array &$items): bool { $items["added"] = 7; return true; }
class ArrayWriter {
    public function write(array &$items): void { $items["method"] = 8; }
}
$direct = [1];
addKey($direct);
echo implode(",", array_keys($direct));
$named = [2];
addKey(items: $named);
echo implode(",", array_keys($named));
$callback = addKey(...);
$callable = [3];
$callback($callable);
echo implode(",", array_keys($callable));
$writer = new ArrayWriter();
$method = [4];
$writer->write(items: $method);
echo implode(",", array_keys($method));
$conditional = [5];
$hash = ["old" => 6];
$argc > 0 && addKey($conditional);
$argc > 0 && addKey($hash);
echo implode(",", array_keys($conditional)), implode(",", array_keys($hash));
$stringKeys = ["keep" => 1, "drop" => 2];
$snapshot = $stringKeys;
addKey($stringKeys);
echo implode(",", array_keys($stringKeys)), implode(",", array_keys($snapshot));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
