//! Purpose:
//! Verifies computed property updates preserve scalar and heap result storage.
//!
//! Called from:
//! - The AST-to-EIR regression suite.
//!
//! Key details:
//! - Ordinary getters and Stringable names coexist with the native XML protocol.

/// Computed updates validate and emit for scalar operands and concrete string getters.
#[test]
fn computed_property_update_storage_on_every_target() {
    let source = r#"<?php
class UpdateOwnerCounter {
    public int $number = 3;
    public string $text = 'a';
    public function receiver(): self { return $this; }
}

class UpdateOwnerName {
    public function __toString(): string { return 'number'; }
}
function updateOwnerExercise(UpdateOwnerCounter $counter): void {
    $counter->receiver()->number += 2;
    echo ++$counter->receiver()->number, $counter->receiver()->number++;
    echo ($counter->receiver()->text .= 'b');
    $counter->{new UpdateOwnerName()} += 1;
    echo $counter->{new UpdateOwnerName()};
}
updateOwnerExercise(new UpdateOwnerCounter());
function updateOwnerNumericMask(int|float $value): int { return $value & 7; }
echo updateOwnerNumericMask($argc);
class UpdateOwnerRegistry {
    public array $handlers = [];
    public mixed $weights = [1.5, 2.5];
    public function step(): void { $this->weights[0]++; --$this->weights[1]; }
    public function registry(): self { return $this; }
}
$registry = new UpdateOwnerRegistry();
$registry->registry()->handlers['save'][] = 'first';
$registry->registry()->handlers['save'][] = 'second';
echo count($registry->handlers['save']);
$registry->step();
$registry->weights[0]++;
echo json_encode($registry->weights);
$xml = new SimpleXMLElement('<r><a>1</a></r>');
$xml->a += 1;
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let numeric = module.functions.iter().find(|function| function.name == "updateOwnerNumericMask").unwrap();
        assert!(!numeric.instructions.iter().any(|inst| matches!(inst.op,
            crate::ir::Op::InstanceOf | crate::ir::Op::PushCallOperandOwner)),
            "{target}: scalar coercion must not gain native dispatch or operand owners\n{}",
            crate::ir::print_function(numeric));
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Erased receivers reach the existing declared-slot COW writer on every supported target.
#[test]
fn erased_property_dimension_uses_declared_storage_on_every_target() {
    let source = "<?php class ErasedWriteStorage { public array $items = [1]; } \
        function mutateErasedStorage(mixed $box): void { $box->items[0] = 7; } \
        mutateErasedStorage(new ErasedWriteStorage());";
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "mutateErasedStorage").unwrap();
        assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::PropGetForWrite),
            "{target}: the runtime class must write its declared slot\n{}", crate::ir::print_function(function));
        assert!(!function.instructions.iter().any(|inst| inst.op == crate::ir::Op::DynamicPropSet),
            "{target}: element writes must not introduce magic setters");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Native XML candidates are excluded while ArrayAccess and buffer slots keep their typed writes.
#[test]
fn erased_property_dimension_eligibility_matches_typed_writers_on_every_target() {
    use crate::ir::{Immediate, Op};
    let source = r#"<?php
class XmlDeclaredArrayWriteStorage extends SimpleXMLElement { public array $items = []; }
class PlainDeclaredArrayWriteStorage { public array $items = []; }
class ObjectDeclaredWriteStorage {
    public ArrayObject $items;
    public function __construct() { $this->items = new ArrayObject([]); }
}
class BufferDeclaredWriteStorage { public function __construct(public buffer<int> $values) {} }
function writeErasedDeclaredItems(mixed $box): void { $box->items[0] = 'x'; }
function writeErasedDeclaredBuffer(mixed $box): void { $box->values[0] = 7; }
writeErasedDeclaredItems(new XmlDeclaredArrayWriteStorage('<r/>'));
writeErasedDeclaredItems(new PlainDeclaredArrayWriteStorage());
writeErasedDeclaredItems(new ObjectDeclaredWriteStorage());
buffer<int> $values = buffer_new<int>(1);
writeErasedDeclaredBuffer(new BufferDeclaredWriteStorage($values)); buffer_free($values);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let items = module.functions.iter().find(|function| function.name == "writeErasedDeclaredItems").unwrap();
        assert!(!items.instructions.iter().any(|inst| inst.op == Op::MixedUnbox
            && inst.result.and_then(|value| items.value(value)).is_some_and(|value|
                matches!(&value.php_type, crate::types::PhpType::Object(class)
                    if class == "XmlDeclaredArrayWriteStorage"))),
            "{target}: native XML properties must not enter the ordinary writer");
        let object_arm = items.blocks.iter().find(|block| block.instructions.iter().any(|id|
            items.instruction(*id).is_some_and(|inst| inst.op == Op::PropGet
                && inst.result.and_then(|value| items.value(value)).is_some_and(|value|
                    matches!(&value.php_type, crate::types::PhpType::Object(class) if class == "ArrayObject")))))
            .expect("declared ArrayObject write arm");
        assert!(!object_arm.instructions.iter().any(|id| items.instruction(*id).is_some_and(|inst|
            inst.op == Op::RuntimeCall && matches!(inst.immediate, Some(Immediate::Data(_))))),
            "{target}: ArrayAccess writes must not use the stdClass-only property fallback");
        let buffer = module.functions.iter().find(|function| function.name == "writeErasedDeclaredBuffer").unwrap();
        assert!(buffer.instructions.iter().any(|inst| inst.op == Op::BufferSet), "{target}: typed buffer write");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Object arithmetic borrows an incdec working slot until overwrite and final cleanup.
#[test]
fn native_dimension_incdec_working_slot_has_one_owner_on_every_target() {
    use crate::ir::{Immediate, LocalKind, Op, ValueDef};
    let source = r#"<?php
function dimensionOwnerStep(SimpleXMLElement $xml): void { $xml[0]++; }
dimensionOwnerStep(new SimpleXMLElement('<r>7</r>'));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "dimensionOwnerStep").unwrap();
        let call = function.instructions.iter().find(|inst| inst.op == Op::StaticMethodCall
            && matches!(inst.immediate, Some(Immediate::Data(name))
                if module.data.strings[name.as_raw() as usize] == "SimpleXMLElement::__elephc_add"))
            .expect("native XML incdec must retain its object arithmetic branch");
        let old = call.operands[0];
        let ValueDef::Instruction { inst, .. } = function.value(old).unwrap().def else {
            panic!("{target}: working operand must load its retained variable");
        };
        let load = function.instruction(inst).unwrap();
        assert_eq!(load.op, Op::LoadLocal, "{target}: incdec working operand");
        let Some(Immediate::LocalSlot(slot)) = load.immediate else { panic!("{target}: working slot"); };
        assert_eq!(function.locals[slot.as_raw() as usize].kind, LocalKind::PhpLocal,
            "{target}: updating storage must not transfer on each load");
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::Release
            && inst.operands.contains(&old)), "{target}: arithmetic must not consume the slot's old owner");
        assert_eq!(function.instructions.iter().filter(|inst| inst.op == Op::ReleaseLocalSlot
            && inst.immediate == Some(Immediate::LocalSlot(slot))).count(), 2,
            "{target}: overwrite retires the old owner and final cleanup retires the replacement");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Static Mixed indexed writes publish an independent box before runtime mutation.
#[test]
fn static_mixed_dimension_write_separates_storage_on_every_target() {
    use crate::ir::{Immediate, Op};
    let source = r#"<?php
class StaticDimensionOwner { public static mixed $items = null; }
function staticDimensionOwnerWrite(): void { StaticDimensionOwner::$items[0] = 5; }
staticDimensionOwnerWrite();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "staticDimensionOwnerWrite").unwrap();
        let clone = function.instructions.iter().find(|inst| inst.op == Op::MixedClone)
            .expect("boxed static writes must separate the shared cell");
        let cloned = clone.result.unwrap();
        assert!(function.instructions.iter().any(|inst| inst.op == Op::StoreStaticProperty
            && inst.operands.contains(&cloned)
            && matches!(inst.immediate, Some(Immediate::Data(name))
                if module.data.strings[name.as_raw() as usize] == "StaticDimensionOwner::items")),
            "{target}: publish the independent cell into original static storage");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Eval-reserved dynamic storage still reaches a native getter after a missing hash entry.
#[test]
fn mixed_magic_hash_miss_emits_getter_on_every_target() {
    let source = r#"<?php
class MixedHashOwner { public function __get(string $name): int { return 30; } }
function mixedHashOwnerRead(mixed $object): mixed { return $object->missing; }
echo mixedHashOwnerRead(new MixedHashOwner());
$source = $argc > 0 ? ';' : ''; eval($source);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        assert!(module.class_infos["MixedHashOwner"].dynamic_property_hash_is_name_addressable(),
            "{target}: the fixture must include the overlapping hash candidate");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let call = if module.target.arch == crate::codegen::platform::Arch::AArch64 { "bl" } else { "call" };
        let getter = crate::names::method_symbol("MixedHashOwner", "__get");
        assert!(assembly.contains("mixed_prop_hash_found"), "{target}: distinguish an actual entry from a missing key");
        assert!(assembly.contains(&format!("{call} {getter}")), "{target}: missing keys reach the normal magic getter");
    }
}

/// Missing hash entries honor borrowed and reference getter return contracts on every target.
#[test]
fn mixed_magic_hash_getter_returns_are_owned_on_every_target() {
    let source = r#"<?php
#[AllowDynamicProperties]
class MixedGetterReferenceOwner {
    private mixed $items = [1, 2];
    public function &__get(string $name): mixed { return $this->items; }
}
#[AllowDynamicProperties]
class MixedGetterBorrowedOwner {
    public static mixed $items = null;
    public function __get(string $name): mixed { return self::$items; }
}
function mixedGetterOwnerRead(mixed $object): mixed { return $object->missing; }
echo json_encode(mixedGetterOwnerRead(new MixedGetterReferenceOwner()));
echo json_encode(mixedGetterOwnerRead(new MixedGetterBorrowedOwner()));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let symbol = crate::names::function_symbol("mixedGetterOwnerRead");
        let function = assembly.split_once(&format!("{symbol}:\n")).unwrap().1;
        let function = function.split("\n.globl ").next().unwrap();
        assert!(function.contains("magic_return_owned"), "{target}: borrowed returns need marker handling");
        let clone = function.find("__rt_mixed_clone").expect("reference returns need value snapshots");
        let release = function.find("__rt_reference_cell_release").expect("reference leases need retirement");
        assert!(clone < release, "{target}: copy the payload before retiring its cell");
    }
}

/// Reference isset probes release their lease before testing the raw bool on every target.
#[test]
fn mixed_magic_hash_reference_isset_uses_bool_payload_on_every_target() {
    let source = r#"<?php
#[AllowDynamicProperties]
final class ReferenceIssetOwner {
    public bool $answer = false;
    public function &__isset(string $name): bool { return $this->answer; }
}
function referenceIssetOwnerProbe(mixed $object): bool { return isset($object->missing); }
var_dump(referenceIssetOwnerProbe(new ReferenceIssetOwner()));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let symbol = crate::names::function_symbol("referenceIssetOwnerProbe");
        let body = assembly.split_once(&format!("{symbol}:\n")).unwrap().1;
        let body = body.split("\n.globl ").next().unwrap();
        let arm = module.target.arch == crate::codegen::platform::Arch::AArch64;
        let call = if arm { "bl" } else { "call" };
        let callee = crate::names::method_symbol("ReferenceIssetOwner", "__isset");
        let after = body.split_once(&format!("{call} {callee}")).unwrap().1;
        let release = after.find("__rt_reference_cell_release").expect("probe must retire the cell");
        let compare = after.find(if arm { "cmp x0, #0" } else { "test rax, rax" })
            .expect("probe must test the returned bool");
        assert!(release < compare, "{target}: restore the bool before the existing probe branch");
    }
}
