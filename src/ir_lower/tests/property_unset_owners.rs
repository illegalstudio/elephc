//! Purpose:
//! Preserves receiver projections across removal of declared property elements.
//!
//! Called from:
//! - The AST-to-EIR ownership regression suite.
//!
//! Key details:
//! - Delayed variable reads can acquire an object owner from final Mixed storage.
//! - The removal and uninitialized no-op paths must both retire the receiver lease.

use crate::ir::{Immediate, Op};

/// Guard-only loads retire before either initialized or uninitialized ArrayAccess branches.
#[test]
fn typed_offset_unset_releases_guard_projection_on_all_targets() {
    let source = r#"<?php
class GuardProjectionBag implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
class GuardProjectionHolder { public GuardProjectionBag $items; }
function unsetGuardProjection(mixed $holder): void {
    $holder = new GuardProjectionHolder();
    unset($holder->items[0]);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "unsetGuardProjection").unwrap();
        let (probe_index, probe) = function.instructions.iter().enumerate().find(|(_, inst)| inst.op == Op::PropInitialized)
            .expect("typed offset removal must test initialization");
        let receiver = probe.operands[0];
        let probe_id = crate::ir::InstId::from_raw(probe_index as u32);
        let block = function.blocks.iter().find(|block| block.instructions.contains(&probe_id)).unwrap();
        let position = block.instructions.iter().position(|id| *id == probe_id).unwrap();
        assert!(block.instructions[position + 1..].iter().any(|id| {
            let inst = function.instruction(*id).unwrap();
            inst.op == Op::Release && inst.operands == [receiver]
        }), "{target}: the guard projection must retire before the conditional branch");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Every ABI roots a projected receiver before probing its property and retires it afterward.
#[test]
fn property_unset_roots_projected_variable_receivers_on_all_targets() {
    let source = r#"<?php
class ProjectedUnsetHolder { public array $items; }
function projectedUnsetProperty(mixed $box): void {
    $box = new ProjectedUnsetHolder();
    unset($box->items[0]);
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "projectedUnsetProperty").unwrap();
        let probe = function.instructions.iter().position(|inst| inst.op == Op::PropInitialized).unwrap();
        let receiver = function.instructions[probe].operands[0];
        let owner = function.instructions[..probe].iter().find_map(|inst| {
            if inst.op == Op::StoreLocal && inst.operands == [receiver] {
                if let Some(Immediate::LocalSlot(slot)) = &inst.immediate { return Some(*slot); }
            }
            None
        }).expect("projected receiver has a dedicated owner slot");
        assert!(function.instructions[..probe].iter().any(|inst|
            inst.op == Op::PushCallOperandOwner && inst.immediate == Some(Immediate::LocalSlot(owner))),
            "{target}: receiver remains rooted during removal");
        assert!(function.instructions[probe + 1..].iter().any(|inst|
            inst.op == Op::ReleaseLocalSlot && inst.immediate == Some(Immediate::LocalSlot(owner))),
            "{target}: initialized and uninitialized paths retire the receiver");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
