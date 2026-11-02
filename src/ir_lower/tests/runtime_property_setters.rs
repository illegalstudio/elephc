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
function runtimeHookAssign(mixed $object, mixed $value): void { $object->value = $value; }
function runtimeHookAssignBase(RuntimeHookBase $object, mixed $value): void { $object->value = $value; }
$object = new RuntimeHookTarget(); runtimeHookAssign($object, 'value');
runtimeHookAssignBase($object, 'derived');
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
        && base.value(inst.operands[0]).unwrap().php_type == PhpType::Mixed),
        "{target}: fallback storage must dispatch the runtime class instead of a missing base slot");
}

supported_target_cases!(runtime_property_setters, verify_runtime_property_setters);
