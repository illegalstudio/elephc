//! Purpose:
//! Checks boxed property setters at the target-independent EIR dispatch boundary.
//!
//! Called from:
//! - The AST-to-EIR supported-target regression suite.
//!
//! Key details:
//! - A runtime class guard must reach a typed accessor instead of only a raw slot store.
//! - Typed extraction keeps the receiver's physical representation and PHP type consistent.

use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// Every target selects the hook through EIR and retains the ordinary storage fallback.
fn verify_runtime_property_setters(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class RuntimeHookTarget {
    public mixed $value = null { set { $this->value = $value; } }
}
function runtimeHookAssign(mixed $object, mixed $value): void { $object->value = $value; }
$object = new RuntimeHookTarget(); runtimeHookAssign($object, 'value');
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "runtimeHookAssign").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == Op::InstanceOf),
        "{target}: boxed receivers require runtime class selection");
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
}

supported_target_cases!(runtime_property_setters, verify_runtime_property_setters);
