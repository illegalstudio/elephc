//! Purpose:
//! Verifies the shared dynamic magic-call ABI and owner publication on every target.
//!
//! Called from:
//! - The AST-to-EIR target regression harness.
//!
//! Key details:
//! - Named arguments and a hidden introspection collector exercise the selected hook path.
//! - The assembly backend must consume normal EIR calls without synthesizing magic owners.

use crate::ir::{Immediate, Op};
use crate::types::PhpType;

/// Magic hooks use boxed array storage and ordinary rooted call operands on every target.
fn verify_method_call_review_contract(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
class ReviewTargetHook {
    public function __call(string $name, array $args): string {
        return $name . $args["value"] . func_num_args();
    }
}
function reviewTargetDispatch(mixed $receiver): mixed {
    return [$receiver->missing(value: "ok")][0];
}
reviewTargetDispatch(new ReviewTargetHook());
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    assert_eq!(module.class_infos["ReviewTargetHook"].methods["__call"].params[1].1,
        PhpType::Mixed, "{target}: named keys need generic PHP array storage");
    let function = module.functions.iter().find(|f| f.name == "reviewTargetDispatch").unwrap();
    let call = function.instructions.iter().find(|inst| inst.op == Op::MethodCall
        && matches!(inst.immediate, Some(Immediate::Data(id))
            if module.data.strings[id.as_raw() as usize] == "__call"))
        .expect("the selected magic hook must be an ordinary EIR method call");
    assert_eq!(function.value(call.operands[2]).unwrap().php_type.codegen_repr(), PhpType::Mixed,
        "{target}: hook args cannot depend on a specialized element type");
    let argument = function.instructions.iter().find(|inst| inst.result == Some(call.operands[2])).unwrap();
    let Some(Immediate::LocalSlot(slot)) = argument.immediate else {
        panic!("{target}: the argument container needs a published local owner");
    };
    assert!(function.instructions.iter().any(|inst| inst.op == Op::PushCallOperandOwner
        && inst.immediate == Some(Immediate::LocalSlot(slot))),
        "{target}: the actual magic argument owner must participate in exceptional cleanup");
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

supported_target_cases!(method_call_review_contract, verify_method_call_review_contract);
