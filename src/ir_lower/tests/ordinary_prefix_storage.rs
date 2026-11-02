//! Purpose:
//! Checks typed scalar prefix readback after native XML dispatch selects an ordinary object.
//!
//! Called from:
//! - The supported-target AST-to-EIR regression suite.
//!
//! Key details:
//! - A coercing string declaration must yield its stored value through a guarded typed read.
//! - Prefix dispatch uses exact classes, leaving getters and eval subclass overrides untouched.

use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Every supported target retains the typed string read after an erased prefix write.
fn verify_ordinary_prefix_storage(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class EirPrefixScalar { public string $value = '5'; }
function eirPrefixScalar(mixed $object): mixed { return ++$object->value; }
$xml = new SimpleXMLElement('<r/>');
var_dump(eirPrefixScalar(new EirPrefixScalar()));
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "eirPrefixScalar").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
        && inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::GetClass)))),
        "{target}: only exact runtime classes may select backing storage");
    assert!(function.instructions.iter().any(|inst| inst.op == Op::PropGet
        && inst.result_php_type == PhpType::Str
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.strings[data.as_raw() as usize] == "value")),
        "{target}: the result must preserve the string property's stored coercion");
}

supported_target_cases!(ordinary_prefix_storage, verify_ordinary_prefix_storage);
