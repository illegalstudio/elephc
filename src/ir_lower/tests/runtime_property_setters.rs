//! Purpose:
//! Checks boxed property setters at the target-independent EIR dispatch boundary.
//!
//! Called from:
//! - The AST-to-EIR supported-target regression suite.
//!
//! Key details:
//! - A runtime class guard must reach a typed accessor instead of only a raw slot store.
//! - Typed extraction keeps the receiver's physical representation and PHP type consistent.

use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Every target selects the hook through EIR and retains the ordinary storage fallback.
/// The base permits property names supplied only by its runtime subclasses.
fn verify_runtime_property_setters(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
#[\AllowDynamicProperties]
class RuntimeHookBase {}
class RuntimeHookTarget extends RuntimeHookBase {
    public mixed $value = null { set { $this->value = $value; } }
}
class RuntimeStringHookTarget {
    public string $text = '' { set { $this->text = $value; } }
}
function runtimeHookAssign(mixed $object, mixed $value): void { $object->value = $value; }
function runtimeStringHookAssign(mixed $object, mixed $value): void { $object->text = $value; }
function runtimeHookAssignBase(RuntimeHookBase $object, string $value): void { $object->value = $value; }
$object = new RuntimeHookTarget(); runtimeHookAssign($object, 'value');
runtimeHookAssignBase($object, 'derived');
runtimeStringHookAssign(new RuntimeStringHookTarget(), 'string');
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "runtimeHookAssign").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
        && inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::GetClass)))),
        "{target}: boxed receivers require exact runtime class selection");
    let setter = function.instructions.iter().find(|inst| inst.op == Op::MethodCall
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.strings[data.as_raw() as usize] == "__propset_value"))
        .expect("a boxed property assignment must call its setter hook");
    let receiver = setter.operands[0];
    assert!(function.instructions.iter().any(|inst| inst.op == Op::MixedUnbox
        && inst.result.is_some_and(|result| function.value(result).unwrap().php_type
            == PhpType::Object("RuntimeHookTarget".into()))),
        "{target}: the selected hook needs a real unboxed receiver");
    assert_eq!(function.value(receiver).unwrap().php_type, PhpType::Object("RuntimeHookTarget".into()));
    assert!(function.instructions.iter().any(|inst| matches!(inst.op, Op::PropSet | Op::DynamicPropSet)),
        "{target}: ordinary runtime classes still need physical storage");
    let base = module.functions.iter().find(|function| function.name == "runtimeHookAssignBase").unwrap();
    assert!(base.instructions.iter().any(|inst| inst.op == Op::MethodCall
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.strings[data.as_raw() as usize] == "__propset_value")),
        "{target}: raw base-class receivers also need the subclass setter arm");
    assert!(base.instructions.iter().any(|inst| inst.op == Op::DynamicPropSet
        && base.value(inst.operands[0]).unwrap().php_type == PhpType::Mixed
        && base.value(inst.operands[2]).unwrap().php_type == PhpType::Mixed),
        "{target}: boxed fallback receivers and values must use the runtime property guard");
    let strings = module.functions.iter().find(|function| function.name == "runtimeStringHookAssign").unwrap();
    let (call_position, setter) = strings.instructions.iter().enumerate().find(|(_, inst)| inst.op == Op::MethodCall
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.strings[data.as_raw() as usize] == "__propset_text")).unwrap();
    assert_eq!(strings.value(setter.operands[1]).unwrap().php_type, PhpType::Str,
        "{target}: setter conversions must be visible to EIR ownership");
    let owner = strings.instructions[..call_position].iter().find_map(|inst| match inst.immediate {
        Some(Immediate::LocalSlot(slot)) if inst.op == Op::StoreLocal
            && inst.operands == [setter.operands[1]] => Some(slot),
        _ => None,
    }).expect("the converted setter string must be stored in an owning slot");
    assert!(strings.instructions[..call_position].iter().any(|inst| inst.op == Op::PushCallOperandOwner
        && inst.immediate == Some(Immediate::LocalSlot(owner))), "{target}: converted string needs an unwind owner");
    for op in [Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
        assert!(strings.instructions[call_position + 1..].iter().any(|inst| inst.op == op
            && inst.immediate == Some(Immediate::LocalSlot(owner))),
            "{target}: converted setter string needs {op:?}");
    }
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(runtime_property_setters, verify_runtime_property_setters);

/// Eval ownership must be probed before a native backing class can claim the declared slot.
fn verify_native_backed_eval_property_dispatch(target: &str) {
    let source = r#"<?php
class NativeEvalStorageBase { public int $count = 1; }
function nativeEvalStorageWrite(mixed $object, int $value): void { $object->count = $value; }
$code = $argc > 0 ? 'class DynamicEvalStorageChild extends NativeEvalStorageBase {} $child = new DynamicEvalStorageChild();' : '';
eval($code);
nativeEvalStorageWrite($child, 2);
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let symbol = crate::names::function_symbol("nativeEvalStorageWrite");
    let function = &assembly[assembly.find(&format!("{symbol}:")).unwrap()..];
    let probe = function.find("__elephc_eval_dynamic_object_property_set").unwrap();
    let native_slot = function.find("mixed_dyn_prop_set_count").unwrap();
    assert!(probe < native_slot, "{target}: eval identity must precede the backing slot's class-id arm");
    let label = function.lines().find(|line| line.trim_end().ends_with(':')
        && line.contains("eval_prop_set_raise")).expect("eval setter throw label");
    let raise = &function[function.find(label).unwrap()..];
    let retain = raise.find("__rt_incref").expect("the native throw owns the Throwable payload");
    let retire = raise[retain..].find("__rt_decref_mixed").expect("the bridge-owned throw box is retired");
    let throw = raise[retain..].find("__rt_throw_current").unwrap();
    assert!(retire < throw, "{target}: retire the box before native unwinding");
}

supported_target_cases!(native_backed_eval_property_dispatch, verify_native_backed_eval_property_dispatch);

/// Runtime property names preserve both accessor calls in the ordinary XML fallback.
fn verify_dynamic_property_hook_updates(target: &str) {
    let source = r#"<?php
class DynamicHookXmlCapability extends SimpleXMLElement {}
class DynamicHookCounter {
    public string $value = '5' {
        get { return $this->value; }
        set { $this->value = $value; }
    }
}
function dynamicHookUpdate(mixed $object, string $name): mixed { return --$object->{$name}; }
function namedHookUpdate(mixed $object): mixed { return ++$object->value; }
$capability = new DynamicHookXmlCapability('<r/>');
echo dynamicHookUpdate(new DynamicHookCounter(), 'value');
unset($capability);
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    for name in ["dynamicHookUpdate", "namedHookUpdate"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        for method in ["__propget_value", "__propset_value"] {
            assert!(function.instructions.iter().any(|instruction| instruction.op == Op::MethodCall
                && matches!(instruction.immediate, Some(Immediate::Data(id))
                    if module.data.strings[id.as_raw() as usize] == method)),
                "{target}: runtime property names must invoke {method}\n{}", crate::ir::print_function(function));
        }
        let (position, setter) = function.instructions.iter().enumerate().find(|(_, instruction)|
            instruction.op == Op::MethodCall && matches!(instruction.immediate,
                Some(Immediate::Data(id)) if module.data.strings[id.as_raw() as usize] == "__propset_value"))
            .unwrap();
        let receiver = setter.operands[0];
        let owner = function.instructions[..position].iter().find_map(|instruction|
            match instruction.immediate {
                Some(Immediate::LocalSlot(slot)) if instruction.op == Op::StoreLocal
                    && instruction.operands == [receiver] => Some(slot),
                _ => None,
            }).expect("the extracted hook receiver must have its own cleanup slot");
        assert!(function.instructions[..position].iter().any(|instruction|
            instruction.op == Op::PushCallOperandOwner
                && instruction.immediate == Some(Immediate::LocalSlot(owner))), "{target}: {name}: receiver unwind owner");
        for op in [Op::PopCallOperandOwner, Op::ReleaseLocalSlot] {
            assert!(function.instructions[position + 1..].iter().any(|instruction|
                instruction.op == op && instruction.immediate == Some(Immediate::LocalSlot(owner))),
                "{target}: {name}: receiver {op:?}");
        }
    }
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(dynamic_property_hook_updates, verify_dynamic_property_hook_updates);

/// A successful class guard reaches its getter through the selected object's real ABI.
fn verify_runtime_property_getter_receivers(target: &str) {
    let source = r#"<?php
class GetterViewBase {
    public string $text = 'base' { get { return $this->text; } }
}
class GetterViewChild extends GetterViewBase {
    public string $text = 'child' { get { return $this->text; } }
}
class GetterViewNoise {
    public function __call(string $name, array $args): mixed { throw new LogicException('foreign call'); }
}
function runtimeHookRead(mixed $object): mixed { return $object->text; }
function runtimeDynamicHookRead(mixed $object, string $name): mixed { return $object->{$name}; }
echo runtimeHookRead(new GetterViewChild());
echo runtimeDynamicHookRead(new GetterViewBase(), 'text');
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    for name in ["runtimeHookRead", "runtimeDynamicHookRead"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let calls = function.instructions.iter().filter(|instruction| instruction.op == Op::MethodCall
            && matches!(instruction.immediate, Some(Immediate::Data(id))
                if module.data.strings[id.as_raw() as usize] == "__propget_text")).collect::<Vec<_>>();
        assert!(!calls.is_empty(), "{target}: {name}: getter dispatch must remain present");
        for call in calls {
            let receiver = function.value(call.operands[0]).unwrap();
            assert!(matches!(&receiver.php_type, PhpType::Object(class)
                if class == "GetterViewBase" || class == "GetterViewChild"),
                "{target}: {name}: guarded getter receiver must be concrete: {:?}", receiver.php_type);
            assert!(function.instructions.iter().any(|instruction| instruction.op == Op::MixedUnbox
                && instruction.result.is_some_and(|value| function.value(value).unwrap().php_type == receiver.php_type)),
                "{target}: {name}: boxed storage needs real extraction, not metadata relabeling");
        }
    }
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(runtime_property_getter_receivers, verify_runtime_property_getter_receivers);

/// A selected XML write-fetch keeps its boxed ABI without another global magic ladder.
fn verify_guarded_native_property_write_fetch(target: &str) {
    let source = r#"<?php
class WriteFetchCell { public mixed $items = []; }
class WriteFetchNoise {
    public function __call(string $name, array $args): mixed { return 'magic:' . $name; }
}
function guardedWriteFetch(mixed $object): void {
    $object->items['next'] = 'value';
    $object->items[] = 'tail';
}
function ordinaryWriteFetchName(mixed $object): mixed {
    return $object->__elephc_property_write_fetch('items');
}
$capability = new SimpleXMLElement('<root/>');
guardedWriteFetch(new WriteFetchCell());
echo ordinaryWriteFetchName(new WriteFetchNoise());
unset($capability);
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "guardedWriteFetch").unwrap();
    let calls = function.instructions.iter().filter(|instruction| instruction.op == Op::MethodCall
        && matches!(instruction.immediate, Some(Immediate::Data(id))
            if module.data.strings[id.as_raw() as usize] == "__elephc_property_write_fetch")).collect::<Vec<_>>();
    assert!(!calls.is_empty(), "{target}: the native write-fetch must remain present");
    assert!(calls.iter().all(|call| function.value(call.operands[0]).unwrap().php_type.codegen_repr() == PhpType::Mixed),
        "{target}: selected XML calls retain their physical boxed receiver");
    assert!(!crate::ir::print_function(function).contains("method.selected.select"),
        "{target}: a guarded XML operation must not repeat global argument planning");
    let ordinary = module.functions.iter().find(|function| function.name == "ordinaryWriteFetchName").unwrap();
    assert!(crate::ir::print_function(ordinary).contains("method.selected.select"),
        "{target}: an ordinary same-name call still needs magic dispatch");
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(guarded_native_property_write_fetch, verify_guarded_native_property_write_fetch);

/// Literal names prune unrelated declared slots while computed names retain every valid arm.
fn verify_literal_mixed_property_write_candidates(target: &str) {
    let source = r#"<?php
class LiteralWriteTarget { public mixed $chosen = null; }
class LiteralWriteOther { public mixed $unrelatedWriteSlot = null; }
function writeKnownMixedProperty(mixed $object): void { $object->chosen = 1; }
function writeUnknownMixedProperty(mixed $object, string $name): void { $object->{$name} = 1; }
writeKnownMixedProperty(new LiteralWriteTarget());
writeUnknownMixedProperty(new LiteralWriteOther(), 'unrelatedWriteSlot');
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    for (name, dynamic) in [("writeKnownMixedProperty", false), ("writeUnknownMixedProperty", true)] {
        let symbol = crate::names::function_symbol(name);
        let body = assembly.split_once(&format!("{symbol}:\n")).unwrap().1;
        let body = body.split("\n.globl ").next().unwrap();
        assert!(body.contains("mixed_dyn_prop_set_chosen_"), "{target}: preserve matching slot dispatch");
        assert_eq!(body.contains("mixed_dyn_prop_set_unrelatedWriteSlot_"), dynamic,
            "{target}: only computed names need unrelated declared candidates");
    }
}

supported_target_cases!(literal_mixed_property_write_candidates, verify_literal_mixed_property_write_candidates);

/// Shared dynamic child probes retain raw-object storage and leave declared slots separate.
fn verify_shared_dynamic_property_children(target: &str) {
    let source = r#"<?php
#[AllowDynamicProperties] class ChildSharingOne { public int $padding = 41; }
#[AllowDynamicProperties] class ChildSharingTwo { private string $padding = 'base'; public bool $flag = true; }
#[AllowDynamicProperties] class ChildSharingThree { public function __debugInfo(): array { return ['visible' => true]; } }
class ChildSharingDeclared { public mixed $items = []; }
function childSharingKeep(mixed $object): mixed { return $object; }
function childSharingProbe(mixed $object): void {
    childSharingKeep($object)->items['next'] = 'value';
}
$capability = new SimpleXMLElement('<root/>');
childSharingProbe(new ChildSharingOne());childSharingProbe(new ChildSharingTwo());
childSharingProbe(new ChildSharingThree());childSharingProbe(new ChildSharingDeclared());
unset($capability);
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "childSharingProbe").unwrap();
    let reads = function.instructions.iter().filter(|inst| inst.immediate == Some(Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::PropertyHashRead)) && function.value(inst.operands[0])
            .is_some_and(|value| value.ir_type == crate::ir::IrType::Heap(crate::ir::IrHeapKind::Object)
                && matches!(&value.php_type, PhpType::Object(class)
                    if class == crate::ir::RUNTIME_OBJECT_VIEW_CLASS)))
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 1, "{target}: all eligible dynamic hashes share one owned child read");
    let object = function.value(reads[0].operands[0]).unwrap();
    assert_eq!(object.ir_type, crate::ir::IrType::Heap(crate::ir::IrHeapKind::Object),
        "{target}: hash protocols require a raw object");
    assert!(matches!(&object.php_type, PhpType::Object(class)
        if class == crate::ir::RUNTIME_OBJECT_VIEW_CLASS),
        "{target}: shared hash reads must retain runtime-layout metadata");
    assert_eq!(object.ownership, crate::ir::Ownership::Borrowed);
    assert!(function.instructions.iter().any(|inst| inst.op == Op::MixedUnbox && inst.result == Some(reads[0].operands[0])),
        "{target}: the boxed receiver must be physically extracted");
    assert!(function.instructions.iter().any(|inst| inst.op == Op::PropInitialized),
        "{target}: declared fields retain independent initialization checks");
    assert_eq!(crate::ir::print_function(function).matches("xml.property_child.dynamic_shared:").count(), 1);
    crate::ir::validate_module(&module).unwrap();
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    assert!(asm.contains("_class_object_dynamic_prop_flags"));
    assert!(asm.contains("_class_object_payload_sizes"));
}

supported_target_cases!(shared_dynamic_property_children, verify_shared_dynamic_property_children);
