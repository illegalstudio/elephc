//! Purpose:
//! Regression coverage for conditional readonly property writes on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR unit suite through `crate::ir_lower::tests`.
//!
//! Key details:
//! - Readonly violations belong to the fallback branch, not the initialized-value branch.
//! - Emission must also support explicit and implicit setter reflection metadata.

use crate::codegen::platform::Target;
use crate::ir::{Op, Terminator};
use std::path::Path;

/// Verifies the readonly fallback graph and target-aware reflection emission.
fn check_conditional_readonly_write(target: &str) {
    let source = r#"<?php
class ConditionalReadonly {
    public(set) readonly int $explicit;
    public(set) readonly ?int $explicitNullable;
    public(set) readonly mixed $explicitMixed;
    public(set) readonly array $explicitArray;
    public(set) readonly ?string $explicitText;
    public readonly int $implicit;
    public readonly ?int $nullable;
}
function fallback(): int { echo "fallback"; return 9; }
function update(ConditionalReadonly $box): void { $box->implicit ??= fallback(); }
function updateNullable(ConditionalReadonly $box): void { $box->nullable ??= fallback(); }
function overwrite(ConditionalReadonly $box): void { $box->implicit = fallback(); }
function initializePublic(ConditionalReadonly $box): void { $box->explicit = fallback(); }
function mixedFallback(): mixed { return "7"; }
function initializeNullablePublic(ConditionalReadonly $box): void {
    $box->explicitNullable = mixedFallback();
}
function initializeMixedPublic(ConditionalReadonly $box): void { $box->explicitMixed = 4; }
function initializeArrayPublic(ConditionalReadonly $box): void { $box->explicitArray = [4]; }
function initializeTextPublic(ConditionalReadonly $box): void { $box->explicitText = 7; }
function overwriteIncompatible(ConditionalReadonly $box): void {
    try { $box->explicit = []; } catch (Error $error) { echo $error->getMessage(); }
}
function initializeUnion(ConditionalReadonly|false $box): void { $box->explicit = fallback(); }
function coalesceUnion(ConditionalReadonly|false $box): void { $box->explicit ??= fallback(); }
function coalesceImplicitUnion(ConditionalReadonly|false $box): void { $box->implicit ??= fallback(); }
class UnionHook { public int $value { get => 7; } }
function coalesceHookUnion(UnionHook|false $box): int { return $box->value ?? 9; }
readonly class LegacyReadonly {
    public $id;
    public function __construct(int $id) { $this->id = $id; }
}
class ReadonlyBase { public readonly int $id; }
class ReadonlyChild extends ReadonlyBase {
    public protected(set) readonly int $id;
    public function __construct() { $this->id = 7; }
}
abstract class AbstractGetter { abstract public int $id { get; } }
class ReadonlyGetter extends AbstractGetter {
    public readonly int $id;
    public function __construct() { $this->id = 42; }
}
abstract class AbstractSetter { abstract public int $id { get; set; } }
abstract class AbstractReadonlyStorage extends AbstractSetter {
    public public(set) readonly int $id;
    public function write(): void { $this->id = 4; }
}
class ReadonlyStorageDescendant extends AbstractReadonlyStorage {}
echo (new ReflectionClass(AbstractReadonlyStorage::class))->isAbstract() ? '' : 'bad';
echo (new ReflectionClass(ReadonlyStorageDescendant::class))->isAbstract() ? 'bad' : '';
(new ReadonlyStorageDescendant())->write();
echo count((new ReflectionProperty(ReadonlyStorageDescendant::class, 'id'))->getHooks());
abstract class AbstractStringSetter { abstract public string $text { set; } }
abstract class AbstractReadonlyStringStorage extends AbstractStringSetter { public public(set) readonly string $text; }
class ReadonlyStringStorageDescendant extends AbstractReadonlyStringStorage {}
interface StorageInterface {} interface DerivedStorageInterface extends StorageInterface {}
class ZStorageValue implements DerivedStorageInterface {}
abstract class AStorageSetter { abstract public ZStorageValue $item { set; } }
abstract class BInterfaceStorage extends AStorageSetter { public public(set) readonly StorageInterface $item; }
class InterfaceStorageDescendant extends BInterfaceStorage {}
function writeAbstractString(mixed $object, mixed $value, string $name): void {
    $object->text = $value;
    $object->{$name} = $value;
}
writeAbstractString(new ReadonlyStringStorageDescendant(), 'value', 'text');
abstract class SetterOnlyBase { abstract public int $item { set; } }
abstract class SetterSupertypeStorage extends SetterOnlyBase { public public(set) readonly int|string $item; }
class SetterStorageDescendant extends SetterSupertypeStorage {}
function reflectAbstractSetter(): void {
    $object = new SetterStorageDescendant();
    $property = new ReflectionProperty(SetterStorageDescendant::class, 'item');
    $property->setValue($object, 4);
    try { $property->setValue($object, 'bad'); } catch (TypeError $error) { echo $error->getMessage(); }
    echo $property->isInitialized($object) ? 'bad' : 'unset';
}
reflectAbstractSetter();
class ReflectedHeapBox { public array $data = ['value']; }
function reflectedHeap(ReflectedHeapBox $box): mixed {
    return (new ReflectionProperty(ReflectedHeapBox::class, 'data'))->getValue($box);
}
echo reflectedHeap(new ReflectedHeapBox())[0];
function acceptAbstractReadonlyStorage(): void {
    eval('abstract class DynamicAbstractReadonlyStorage extends AbstractSetter { public public(set) readonly int $id; }');
}
class NativeMutableParent { public int $id; }
function rejectReadonlyAddition(): void {
    eval('class DynamicReadonlyChild extends NativeMutableParent { public public(set) readonly int $id; }');
}
function receiver(): ConditionalReadonly { echo "receiver"; return new ConditionalReadonly(); }
function updateTemporary(): void { receiver()->implicit ??= fallback(); }
$box = new ConditionalReadonly();
$legacy = new LegacyReadonly(7);
echo $legacy->id;
echo (new ReadonlyGetter())->id;
try { update($box); } catch (Error $e) { echo "error"; }
$properties = (new ReflectionClass(ConditionalReadonly::class))->getProperties(ReflectionProperty::IS_PROTECTED_SET);
echo count($properties);
"#;
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
    );
    let storage = &module.class_infos["ReadonlyStorageDescendant"];
    assert!(!storage.abstract_properties.contains("id"), "{target}: the storage property is concrete");
    let contract = &storage.abstract_property_hooks["id"];
    assert!(contract.get_type.is_none() && contract.set_type.is_some(),
        "{target}: only the inherited setter remains abstract");
    assert_eq!(contract.declaring_type, "AbstractSetter", "{target}: the hook keeps its original owner");
    let reflected = module.functions.iter().find(|function| function.name == "reflectAbstractSetter").unwrap();
    assert!(reflected.instructions.iter().any(|instruction| instruction.op == Op::PropSet
        && matches!(instruction.immediate, Some(crate::ir::Immediate::ReflectionPropertyRef { .. }))),
        "{target}: the reflected write keeps its proven physical target and hook dispatch");
    for name in ["update", "updateNullable"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let insert = function.blocks.iter().find(|block| block.name == "coalesce_assign.default").unwrap();
        let Some(Terminator::CondBr { then_target, else_target, .. }) = &insert.terminator else {
            panic!("{target}: fallback checks initialization before choosing its Error");
        };
        for destination in [then_target, else_target] {
            let arm = function.blocks.iter().find(|block| block.id == *destination).unwrap();
            assert!(matches!(arm.terminator, Some(Terminator::Throw { .. })),
                "{target}: initialized overwrite and inaccessible initialization both throw");
        }
        assert!(insert.instructions.iter().any(|id| function.instruction(*id).unwrap().op == Op::PropInitialized),
            "{target}: a fallback may target an initialized null readonly slot");
        let merge = function.blocks.iter().find(|block| block.name == "coalesce_assign.merge").unwrap();
        assert!(!matches!(merge.terminator, Some(Terminator::Throw { .. })), "{target}: keep does not throw");
        let probe = function.blocks.iter().find(|block| block.name == "coalesce.property.merge").unwrap();
        assert!(probe.instructions.iter().any(|id| function.instruction(*id).unwrap().op == Op::UnsetLocal),
            "{target}: the consumed probe cannot remain in its owner slot");
    }
    let overwrite = module.functions.iter().find(|function| function.name == "overwrite").unwrap();
    assert!(overwrite.blocks.iter().any(|block| matches!(block.terminator,
        Some(Terminator::Throw { .. }))), "{target}: direct readonly writes remain catchable");
    let incompatible = module.functions.iter().find(|function| function.name == "overwriteIncompatible").unwrap();
    for op in [Op::MixedBox, Op::PropInitialized, Op::PropSet] {
        assert!(incompatible.instructions.iter().any(|instruction| instruction.op == op),
            "{target}: incompatible readonly writes preserve runtime initialization and type guards");
    }
    let nullable_store = module.functions.iter().find(|function| function.name == "initializeNullablePublic").unwrap();
    let store_index = nullable_store.instructions.iter().position(|instruction| instruction.op == Op::PropSet).unwrap();
    assert!(nullable_store.instructions[store_index + 1..].iter().any(|instruction| instruction.op == Op::Release),
        "{target}: inline nullable storage releases the boxed source after copying its payload");
    for name in ["initializeMixedPublic", "initializeArrayPublic", "initializeTextPublic"] {
        let store = module.functions.iter().find(|function| function.name == name).unwrap();
        let write_index = store.instructions.iter().position(|instruction| instruction.op == Op::PropSet).unwrap();
        assert!(store.instructions[write_index + 1..].iter().any(|instruction| instruction.op == Op::Release),
            "{target}: {name} retires the successful boxed source, not just its error sibling");
    }
    let union = module.functions.iter().find(|function| function.name == "initializeUnion").unwrap();
    assert!(union.instructions.iter().any(|inst| inst.op == Op::TypePredicate
        && inst.immediate == Some(crate::ir::Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object))),
        "{target}: non-object receivers are rejected before inspecting readonly state");
    for name in ["coalesceUnion", "coalesceImplicitUnion"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        assert!(function.blocks.iter().any(|block| block.name == "coalesce.property.uninitialized"),
            "{target}: a boxed union coalesce must suppress the uninitialized value read");
        assert!(function.instructions.iter().any(|inst| inst.op == Op::PropInitialized),
            "{target}: object and scalar arms use the safe runtime initialization probe");
    }
    let hooked = module.functions.iter().find(|function| function.name == "coalesceHookUnion").unwrap();
    assert!(!hooked.instructions.iter().any(|inst| inst.op == Op::PropInitialized),
        "{target}: virtual union getters never probe their uninitialized backing slot");
    assert!(hooked.instructions.iter().any(|inst| inst.op == Op::MixedUnbox),
        "{target}: the guarded getter borrows no boxed cell as an object pointer");
    for function in [
        module.functions.iter().find(|function| function.name == "initializePublic").unwrap(),
        module.functions.iter().find(|function| function.name == "initializeUnion").unwrap(),
        module.class_methods.iter().find(|function| function.name == "ReadonlyChild::__construct").unwrap(),
        module.class_methods.iter().find(|function| function.name == "ReadonlyGetter::__construct").unwrap(),
    ] {
        assert!(function.instructions.iter().any(|inst| inst.op == Op::PropInitialized), "{target}");
        let initialize = function.blocks.iter().find(|block| block.name == "readonly.write.uninitialized").unwrap();
        assert!(initialize.instructions.iter().any(|id| function.instruction(*id).unwrap().op == Op::PropSet),
            "{target}: authorized first initialization publishes the property");
        let overwrite = function.blocks.iter().find(|block| block.name == "readonly.write.initialized").unwrap();
        assert!(matches!(overwrite.terminator, Some(Terminator::Throw { .. })),
            "{target}: authorized setters still reject an initialized readonly slot");
    }
    let legacy = module.class_methods.iter().find(|function| function.name == "LegacyReadonly::__construct").unwrap();
    assert!(!legacy.instructions.iter().any(|inst| inst.op == Op::PropInitialized),
        "{target}: the legacy untyped constructor initializes an implicitly null slot");
    let temporary = module.functions.iter().find(|function| function.name == "updateTemporary").unwrap();
    assert!(temporary.blocks.iter().flat_map(|block| &block.instructions)
        .any(|id| temporary.instruction(*id).unwrap().op == Op::PushCallOperandOwner),
        "{target}: a throwing fallback must root the temporary receiver");
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Conditional readonly writes and reflection metadata emit on macOS ARM64.
#[test]
fn conditional_readonly_write_macos() { check_conditional_readonly_write("macos-aarch64"); }

/// Conditional readonly writes and reflection metadata emit on iOS devices.
#[test]
fn conditional_readonly_write_ios_device() { check_conditional_readonly_write("ios-arm64"); }

/// Conditional readonly writes and reflection metadata emit on the iOS Simulator.
#[test]
fn conditional_readonly_write_ios_simulator() { check_conditional_readonly_write("ios-sim-arm64"); }

/// Conditional readonly writes and reflection metadata emit on Linux ARM64.
#[test]
fn conditional_readonly_write_linux_arm64() { check_conditional_readonly_write("linux-aarch64"); }

/// Conditional readonly writes and reflection metadata emit on Linux x86_64.
#[test]
fn conditional_readonly_write_linux_x86_64() { check_conditional_readonly_write("linux-x86_64"); }
