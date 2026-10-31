//! Purpose:
//! Verifies declared-slot initialization and nested property RW guards on every target.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Concrete nonnullable slots only probe initialization before a write.
//! - Nested updates select a property in RW mode before their first element read.

use crate::ir::Op;

/// Fused property appends perform a W initialization fetch before probing an uninitialized array.
#[test]
fn nested_property_append_initializes_nullable_array_before_read_on_every_target() {
    let source = r#"<?php
class NestedAppendUninitializedArray { public ?array $items; }
function initializeNestedAppendArray(mixed $box): void { $box->items[0][] = 'x'; }
initializeNestedAppendArray(new NestedAppendUninitializedArray());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "initializeNestedAppendArray").unwrap();
        let initialized = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized)
            .expect("a nested W append must probe initialization before reading its declared root");
        let read = function.instructions.iter().position(|inst| matches!(inst.op, Op::PropGet | Op::PropGetForWrite))
            .expect("the root property must be read after initialization");
        assert!(initialized < read, "{target}: nested append read precedes initialization");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Concrete XML parents never create a raw-object call to an array-only writable-cell helper.
#[test]
fn nested_native_object_parent_selection_emits_on_every_target() {
    let source = r#"<?php
class NativeParentShapes { public mixed $items; }
function opaqueParentShapes(mixed $value): mixed { return $value; }
$xml = new SimpleXMLElement('<r><item>7</item></r>');
$xml->item[0][0]++; ++$xml->item[0][0];
$xml->item[0][0]--; --$xml->item[0][0];
$holder = new NativeParentShapes(); $holder->items = [[1]];
$holder->items[0][0]++;
$cells = opaqueParentShapes([[1]]); $cells[0][0]++;
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(name).unwrap());
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Concrete write fetches probe initialization without acquiring an impossible null payload.
#[test]
fn concrete_property_initialization_guards_on_every_target() {
    let source = r#"<?php
class RequiredFetchSlot { public ArrayObject $items; }
function concreteRequiredWrite(RequiredFetchSlot $box): void { $box->items[0] = 1; }
concreteRequiredWrite(new RequiredFetchSlot());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|f| f.name == "concreteRequiredWrite").unwrap();
        let probe = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized)
            .expect("a concrete slot requires a write initialization probe");
        let read = function.instructions.iter().position(|inst| inst.op == Op::PropGet)
            .expect("an initialized ArrayAccess object retains its ordinary read");
        assert!(probe < read, "{target}: initialization must precede the property read");
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::IsNull),
            "{target}: initialized concrete object storage cannot contain null");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// All nested update forms reject null before element reads and retain RW uninitialized errors.
#[test]
fn nested_property_readwrite_guards_on_every_target() {
    let source = r#"<?php
class NestedGuardSlot { public ?ArrayObject $items; }
function nestedGuardCompound(mixed $box): void { $box->items[0][1] += 1; }
function nestedGuardIncrement(mixed $box): void { $box->items[0][1]++; }
function nestedGuardExpression(mixed $box): mixed { return ($box->items[0][1] += 1); }
function nestedGuardPrefix(mixed $box): mixed { return ++$box->items[0][1]; }
$box = new NestedGuardSlot();
nestedGuardCompound($box); nestedGuardIncrement($box);
nestedGuardExpression($box); nestedGuardPrefix($box);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["nestedGuardCompound", "nestedGuardIncrement", "nestedGuardExpression", "nestedGuardPrefix"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            let guard = function.instructions.iter().position(|inst| inst.op == Op::IsNull)
                .unwrap_or_else(|| panic!("{target}: {name} requires a null guard"));
            let read = function.instructions.iter().position(|inst| matches!(inst.op,
                Op::RuntimeCall | Op::MethodCall | Op::ArrayGet | Op::ArrayGetMixedKey | Op::HashGet)
                && inst.result.is_some() && inst.operands.len() >= 2
                && inst.result_php_type.codegen_repr() == crate::types::PhpType::Mixed)
                .unwrap_or_else(|| panic!("{target}: {name} must retain its element read"));
            assert!(guard < read, "{target}: {name} must guard before element reads");
            let write = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized);
            assert!(write.is_none_or(|write| guard < write),
                "{target}: {name} must select RW before a later write fetch");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Concrete object containers keep offsetGet lowering on locals, this and expression updates.
#[test]
fn concrete_object_property_nested_updates_on_every_target() {
    let source = r#"<?php
class UnitNestedObjectAccess implements ArrayAccess {
    public ArrayObject $child;
    public function __construct() { $this->child = new ArrayObject([1 => 5]); }
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return $this->child; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class ObjectFetchRoots {
    public UnitNestedObjectAccess $custom;
    public SplFixedArray $rows;
    public ArrayObject $data;
    public function __construct() {
        $this->rows = new SplFixedArray(1); $this->rows[0] = [1 => 5];
        $this->data = new ArrayObject(['k' => [1]]); $this->custom = new UnitNestedObjectAccess();
    }
    public function step(): void {
        $this->rows[0][1] += 2;
        $result = ($this->data['k'][0] += 4);
        $this->rows[0][1]++;
        $this->custom[0][1] += 2; $result = ($this->custom[0][1] += 2); $this->custom[0][1]++;
    }
}
$holder = new ObjectFetchRoots();
$holder->rows[0][1] += 2;
$result = ($holder->data['k'][0] += 4);
$holder->rows[0][1]++;
$holder->custom[0][1] += 2; $result = ($holder->custom[0][1] += 2); $holder->custom[0][1]++;
$holder->step();
function nullableObjectFetchRoot(?ObjectFetchRoots $holder): void { $holder->rows[0][1] += 2; }
nullableObjectFetchRoot($argc > 0 ? $holder : null);
function computedObjectFetchRoot(): ObjectFetchRoots { return new ObjectFetchRoots(); }
$result = (computedObjectFetchRoot()->rows[0][1] += 2);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Concrete inferred array slots retain a loud refusal instead of entering a boxed runtime ABI.
#[test]
fn concrete_inferred_property_nested_update_stays_refused() {
    let source = "<?php class InferredFetchRoot { public $grid = [[0]]; } \
        $box = new InferredFetchRoot(); $box->grid[0][0] += 1;";
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let error = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .expect_err("concrete property arrays need an explicit storeback protocol");
        assert!(format!("{error:?}").contains("runtime_call array set with receiver PHP type Array"),
            "{target}: {error:?}");
    }
}

/// W initialization publishes a legal array before the normal fetch while RW remains a read.
#[test]
fn property_array_autovivification_modes_on_every_target() {
    let source = r#"<?php
class InitFetchRoot { public ?array $items; }
function initFetchWrite(InitFetchRoot $box): void { $box->items[0] = 1; }
function initFetchReadWrite(InitFetchRoot $box): void { $box->items[0] += 1; }
initFetchWrite(new InitFetchRoot()); initFetchReadWrite(new InitFetchRoot());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let write = module.functions.iter().find(|f| f.name == "initFetchWrite").unwrap();
        let store = write.instructions.iter().position(|inst| inst.op == Op::PropSet)
            .expect("W fetch must initialize a legal array-bearing slot");
        let fetch = write.instructions.iter().position(|inst| inst.op == Op::PropGetForWrite)
            .expect("W fetch retains its normal attached COW writer");
        assert!(store < fetch, "{target}: initialization must precede the mutable fetch");
        let read = module.functions.iter().find(|f| f.name == "initFetchReadWrite").unwrap();
        let first_read = read.instructions.iter().position(|inst| inst.op == Op::PropGet)
            .expect("RW fetch retains an ordinary property read");
        assert!(read.instructions.iter().position(|inst| inst.op == Op::PropSet)
            .is_none_or(|store| first_read < store), "{target}: RW cannot initialize before reading");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Erased append selects attached declared or hash cells on every supported ABI.
#[test]
fn erased_property_append_uses_attached_storage_on_every_target() {
    let source = r#"<?php
interface AttachedAppendMarker {}
class AttachedAppendDeclared implements AttachedAppendMarker { public array $items = []; }
#[AllowDynamicProperties]
class AttachedAppendDynamic implements AttachedAppendMarker {}
#[AllowDynamicProperties]
class AttachedAppendStdChild extends stdClass {}
#[AllowDynamicProperties]
class AttachedAppendUnrelated {}
class AttachedAppendDeprecated {}
function attachedAppendMixed(mixed $box): void { $box->items[] = 1; }
function attachedAppendObject(object $box): void { $box->items[] = 2; }
function attachedAppendInterface(AttachedAppendMarker $box): void { $box->items[] = 3; }
function attachedAppendKnown(stdClass $box): void { $box->items[] = 4; }
function attachedAppendDeclared(AttachedAppendDeclared $box): void { $box->items[] = 5; }
function attachedAppendDeprecated(mixed $box): void { $box->items[] = 6; }
attachedAppendMixed(new AttachedAppendDeclared());
attachedAppendObject(new stdClass());
attachedAppendInterface(new AttachedAppendDynamic());
attachedAppendKnown(new AttachedAppendStdChild());
attachedAppendDeclared(new AttachedAppendDeclared());
$plain = new AttachedAppendDeprecated(); $reserved = clone($plain, []);
attachedAppendDeprecated($plain);
echo get_class(new AttachedAppendUnrelated());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["attachedAppendMixed", "attachedAppendObject", "attachedAppendInterface"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            assert!(function.instructions.iter().any(|inst| inst.op == Op::PropGetForWrite),
                "{target}: {name} must select the declared array slot");
            assert!(function.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::PropertyHashFetchForWrite))),
                "{target}: {name} must select an attached dynamic cell");
        }
        let direct = module.functions.iter().find(|f| f.name == "attachedAppendDeclared").unwrap();
        assert!(direct.instructions.iter().any(|inst| inst.op == Op::PropGetForWrite),
            "{target}: known declared array must keep its attached direct writer");
        assert!(!direct.instructions.iter().any(|inst| matches!(inst.immediate,
            Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::GetClass)
                | crate::ir::RuntimeCallTarget::PropertyHashFetchForWrite)))),
            "{target}: known declared array must not emit a runtime class ladder or hash arm");
        let known = module.functions.iter().find(|f| f.name == "attachedAppendKnown").unwrap();
        let classes: Vec<_> = known.instructions.iter().filter_map(|inst| match inst.immediate {
            Some(crate::ir::Immediate::Data(id)) if inst.op == Op::ConstStr =>
                Some(module.data.strings[id.as_raw() as usize].as_str()),
            _ => None,
        }).collect();
        for class in ["stdClass", "AttachedAppendStdChild"] {
            assert!(classes.contains(&class), "{target}: known receiver must retain {class}");
        }
        for class in ["AttachedAppendDynamic", "AttachedAppendUnrelated", "AttachedAppendDeclared", "AttachedAppendDeprecated"] {
            assert!(!classes.contains(&class), "{target}: known receiver must exclude unrelated {class}");
        }
        let deprecated = &module.class_infos["AttachedAppendDeprecated"];
        assert!(deprecated.dynamic_property_hash_is_name_addressable()
            && deprecated.dynamic_property_creation_is_deprecated(),
            "{target}: clone-with must reserve a deprecated ordinary hash");
        let write = module.functions.iter().find(|f| f.name == "attachedAppendDeprecated").unwrap();
        assert!(write.instructions.iter().any(|inst| inst.op == Op::ConstStr && matches!(inst.immediate,
            Some(crate::ir::Immediate::Data(id)) if module.data.strings[id.as_raw() as usize] == "AttachedAppendDeprecated")),
            "{target}: mixed writer must contain the deprecated class arm");
        assert!(write.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::PropertyHashFetchForWrite))),
            "{target}: deprecated hash case must reach the attached fetch");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Declared fields in magic classes retain a write initialization probe on every target.
#[test]
fn magic_property_dimension_guards_on_every_target() {
    let source = r#"<?php
class MagicAttachedArray {
    private array $items;
    public function __get(string $name): mixed { return null; }
    public function add(): void { $this->items[] = 1; }
}
class MagicAttachedNull {
    public ?ArrayObject $items = null;
    public function __set(string $name, mixed $value): void {}
}
function magicAttachedWrite(mixed $box): void { $box->items[0] = 1; }
(new MagicAttachedArray())->add(); magicAttachedWrite(new MagicAttachedNull());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let add = module.class_methods.iter().find(|f| f.name == "MagicAttachedArray::add").unwrap();
        assert!(add.instructions.iter().any(|inst| inst.op == Op::PropInitialized),
            "{target}: accessible magic-class slot must retain its initialization probe");
        assert!(add.instructions.iter().any(|inst| inst.op == Op::PropSet),
            "{target}: first append must initialize the declared array");
        let write = module.functions.iter().find(|f| f.name == "magicAttachedWrite").unwrap();
        assert!(write.instructions.iter().any(|inst| inst.op == Op::IsNull),
            "{target}: nullable object slot in a magic class must reject array initialization");
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}

/// Existing getter-class entries and explicit missing-getter refusals lower on every ABI.
#[test]
fn magic_property_append_fetches_on_every_target() {
    let source = r#"<?php
interface MagicAppendTarget {}
#[AllowDynamicProperties]
class MagicAppendSet implements MagicAppendTarget {
    public function __set(string $name, mixed $value): void { $this->$name = $value; }
}
#[AllowDynamicProperties]
class MagicAppendValue implements MagicAppendTarget {
    public function __get(string $name): mixed { return [1]; }
}
class MagicAppendReference implements MagicAppendTarget {
    public array $backing = [1];
    public function &__get(string $name): mixed { return $this->backing; }
}
function magicAppendMixed(mixed $box): void { $box->items[] = 2; }
function magicAppendObject(object $box): void { $box->items[] = 2; }
function magicAppendInterface(MagicAppendTarget $box): void { $box->items[] = 2; }
magicAppendMixed(new MagicAppendSet());
magicAppendObject(new MagicAppendValue());
magicAppendInterface(new MagicAppendReference());
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["magicAppendMixed", "magicAppendObject", "magicAppendInterface"] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            assert!(function.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::PropertyHashContains))),
                "{target}: {name} must distinguish a stored null from a missing getter entry");
            assert!(function.instructions.iter().any(|inst| inst.op == Op::ConstStr && matches!(inst.immediate,
                Some(crate::ir::Immediate::Data(id)) if module.data.strings[id.as_raw() as usize] ==
                    "Unsupported property append through an overloaded __get()")),
                "{target}: {name} must explicitly diagnose missing getter storage");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
