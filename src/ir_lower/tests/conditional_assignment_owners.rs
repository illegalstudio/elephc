//! Purpose:
//! Checks owner retirement after control-flow merges with conditional assignments.
//!
//! Called from:
//! - The AST-to-EIR lowering unit suite.
//!
//! Key details:
//! - Definite initialization is not required for a frame slot to hold an owner.
//! - Every supported target must retire the previous occupant before replacement.

use crate::ir::{Immediate, Op};

/// A later conditional store retires the slot initialized only by an earlier conditional.
#[test]
fn overlapping_conditionals_retire_local_slots_on_all_targets() {
    let source = r#"<?php
function conditional_owner(int $flags): void {
    if (($flags & 1) !== 0) { $text = str_repeat('a', 80); }
    if (($flags & 2) !== 0) { $text = str_repeat('b', 90); }
    if ($flags !== 0) { echo $text; }
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "conditional_owner").unwrap();
        let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("text")).unwrap().id;
        let stores = function.blocks.iter().flat_map(|block| {
            block.instructions.iter().enumerate().filter_map(move |(index, id)| {
                let inst = function.instruction(*id).unwrap();
                (inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot)))
                    .then_some((block, index))
            })
        }).collect::<Vec<_>>();
        assert_eq!(stores.len(), 2, "{name}");
        let (block, index) = stores[1];
        assert!(block.instructions[..index].iter().any(|id| {
            let inst = function.instruction(*id).unwrap();
            inst.op == Op::ReleaseLocalSlot && inst.immediate == Some(Immediate::LocalSlot(slot))
        }), "{name}: the first branch may already own a string in the shared slot");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
