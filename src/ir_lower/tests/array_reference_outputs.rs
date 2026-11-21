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
$conditionalChanged = $argc > 0 && addKey($conditional);
$hashChanged = $argc > 0 && addKey($hash);
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

/// A conditional write through a declared array reference pins the stable published Mixed cell
/// before mutating either packed or keyed payloads.
#[test]
fn conditional_php_array_reference_writes_keep_the_attached_cell() {
    use crate::ir::Op;

    let source = r#"<?php
function conditionallyWriteArray(array &$items): bool { $items["added"] = 7; return true; }
function checkConditionalArray(bool $write): void {
    $packed = [1];
    $hash = ["old" => 2];
    $packedChanged = $write && conditionallyWriteArray($packed);
    $hashChanged = $write && conditionallyWriteArray($hash);
}
checkConditionalArray($argc > 0);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let callee = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("conditionallyWriteArray"))
            .unwrap();
        let pinned = callee.instructions.windows(2).position(|pair| {
            pair[0].op == Op::LoadRefCell
                && pair[1].op == Op::Acquire
                && pair[1].immediate == Some(crate::ir::Immediate::Bool(true))
                && pair[0].result.is_some_and(|result| pair[1].operands == [result])
        })
            .unwrap_or_else(|| panic!("{name}: the published caller cell must be pinned before mutation"));
        // The parameter's entry type guard (php's RECV check) formats its TypeError through
        // get_class() / gettype() on its failure path; those calls are not the write.
        let setter = callee.instructions.iter().position(|inst| {
            inst.op == Op::RuntimeCall
                && !matches!(
                    inst.immediate,
                    Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                        crate::ir::RuntimeFnId::GetClass | crate::ir::RuntimeFnId::Gettype
                    )))
                )
        })
            .unwrap_or_else(|| panic!("{name}: missing boxed array setter"));
        assert!(
            pinned < setter,
            "{name}: the published cell must be pinned before the conditional write",
        );
        assert!(
            !callee.instructions.iter().any(|inst| inst.op == Op::StoreRefCell),
            "{name}: the stable published cell must be mutated in place",
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
