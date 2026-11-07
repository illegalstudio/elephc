//! Purpose:
//! Checks retirement of operator-merge boxes converted to scalar user-call parameters.
//!
//! Called from:
//! - The all-target AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - An Unknown return summary cannot alias an argument box never passed to the callee.

use crate::ir::{Immediate, Op};

/// All targets retire the owned Mixed operand after passing only its scalar copy.
fn verify_scalar_parameter_owners(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function scalarOwnerPick(string $text, int $index): string { return substr($text, $index, 1); }
function scalarOwnerWalk(string $text, mixed $index): string { return scalarOwnerPick($text, $index + 3); }
$xml = new SimpleXMLElement('<r/>'); echo scalarOwnerWalk('abcdefgh', 2);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "scalarOwnerWalk").unwrap();
    let (index, call) = function.instructions.iter().enumerate().find(|(_, inst)| inst.op == Op::Call
        && matches!(inst.immediate, Some(Immediate::Data(data))
            if module.data.function_names[data.as_raw() as usize] == "scalarOwnerPick")).unwrap();
    let operand = call.operands[1];
    assert!(function.instructions[index + 1..].iter().any(|inst|
        inst.op == Op::Release && inst.operands == [operand]),
        "{target}: scalar conversion borrows the caller's Mixed box");
}

supported_target_cases!(scalar_parameter_owners, verify_scalar_parameter_owners);
