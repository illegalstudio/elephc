//! Purpose:
//! Verifies that a packed list bound to a declared hash parameter is rebuilt as that hash by
//! the caller, on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test suite.
//!
//! Key details:
//! - By value the argument is converted on a reference of its own, leaving the caller's list.
//!   The callee writes its parameter so the parameter keeps typed hash storage at the ABI.
//! - By reference the local itself is converted and stored back before its address is passed,
//!   so the callee never reads a packed vector as a hash table.
//! - The emitted assembly is generated too, so every target's backend accepts the shape.

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op};
use std::path::Path;

/// Returns the ops of `function` in block order, with each instruction's operands.
fn instructions<'a>(
    function: &'a crate::ir::Function,
) -> Vec<&'a crate::ir::Instruction> {
    function.blocks.iter()
        .flat_map(|block| block.instructions.iter())
        .map(|id| function.instruction(*id).unwrap())
        .collect()
}

/// Both bindings convert the list in the caller before the call on every target.
#[test]
fn list_bound_to_hash_parameter_is_converted_by_the_caller_on_every_target() {
    let source = r#"<?php
function read(array<int, int> $m): int { $m[9] = 7; return count($m) + $m[0]; }
function put(array<int, int> &$m): void { $m[9] = 7; }
function byValue(): int { $ids = [4, 5]; $read = read($ids); return $read; }
function byReference(): int { $ids = [4, 5]; put($ids); return count($ids); }
echo byValue(), byReference();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );

        let by_value = module.functions.iter().find(|function| function.name == "byValue").unwrap();
        let ops = instructions(by_value);
        let convert = ops.iter().position(|inst| inst.op == Op::ArrayToHash)
            .unwrap_or_else(|| panic!("{name}: the by-value list is rebuilt as a hash"));
        let call = ops.iter().position(|inst| inst.op == Op::Call).unwrap();
        assert!(convert < call, "{name}: the conversion precedes the call");
        let source_value = ops[convert].operands[0];
        let lends_reference = ops[..convert].iter()
            .any(|inst| inst.op == Op::Acquire && inst.result == Some(source_value));
        assert!(lends_reference, "{name}: the conversion consumes a reference of its own");

        let by_reference = module.functions.iter()
            .find(|function| function.name == "byReference").unwrap();
        let ops = instructions(by_reference);
        let convert = ops.iter().position(|inst| inst.op == Op::ArrayToHash)
            .unwrap_or_else(|| panic!("{name}: the by-reference local is promoted to a hash"));
        let call = ops.iter().position(|inst| inst.op == Op::Call).unwrap();
        let loaded = ops[..convert].iter().rev()
            .find(|inst| inst.result == Some(ops[convert].operands[0])).unwrap();
        let Some(Immediate::LocalSlot(slot)) = loaded.immediate else {
            panic!("{name}: the promotion converts the local itself, not a copy");
        };
        assert_eq!(loaded.op, Op::LoadLocal, "{name}: the promotion reads the local");
        let stored_back = ops[convert..call].iter().any(|inst| inst.op == Op::StoreLocal
            && inst.immediate == Some(Immediate::LocalSlot(slot))
            && inst.operands.first().copied() == ops[convert].result);
        assert!(stored_back, "{name}: the hash is stored into the local before the call");

        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
