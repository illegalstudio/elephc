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
fn verify_runtime_property_setters(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
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
echo dynamicHookUpdate(new DynamicHookCounter(), 'value');
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "dynamicHookUpdate").unwrap();
    for method in ["__propget_value", "__propset_value"] {
        assert!(function.instructions.iter().any(|instruction| instruction.op == Op::MethodCall
            && matches!(instruction.immediate, Some(Immediate::Data(id))
                if module.data.strings[id.as_raw() as usize] == method)),
            "{target}: runtime property names must invoke {method}\n{}", crate::ir::print_function(function));
    }
    crate::ir::validate_module(&module).unwrap();
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(dynamic_property_hook_updates, verify_dynamic_property_hook_updates);
