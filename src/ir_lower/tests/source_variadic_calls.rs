//! Purpose:
//! Checks source variadic collector planning when a boxed receiver has no common signature.
//!
//! Called from:
//! - The all-target AST-to-EIR call regression suite.
//!
//! Key details:
//! - Conflicting native signatures exercise the opaque path without lowering an eval catalog.
//! - Exact class checks retain the fallback while typed arms pass a real collector array.

use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Every target plans collectors after runtime class selection instead of passing a raw value.
fn verify_source_variadic_calls(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class SelectedVariadicTarget {
    public function collect(int ...$values): void {}
}
class SelectedFixedTarget {
    public function collect(int $value): void {}
}
function selectedVariadicCall(mixed $object): mixed { return $object->collect(1); }
selectedVariadicCall(new SelectedVariadicTarget());
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "selectedVariadicCall").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
        && inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::GetClass)))),
        "{target}: select an exact runtime class before binding the arguments");
    let selected = function.instructions.iter().find(|inst| inst.op == Op::MethodCall
        && function.value(inst.operands[0]).unwrap().php_type == PhpType::Object("SelectedVariadicTarget".into()))
        .expect("the known variadic class needs its own typed call arm");
    assert!(matches!(function.value(selected.operands[1]).unwrap().php_type.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }),
        "{target}: the callee receives a collector rather than its first source argument");
    assert!(function.instructions.iter().any(|inst| inst.op == Op::MethodCall
        && function.value(inst.operands[0]).unwrap().php_type.codegen_repr() == PhpType::Mixed),
        "{target}: unknown and non-variadic classes retain their existing dispatch");
}

supported_target_cases!(source_variadic_calls, verify_source_variadic_calls);
