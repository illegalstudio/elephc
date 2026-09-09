//! Purpose:
//! Checks explicit heap reference-cell ownership in lowered EIR and target assembly.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Heap cell owners are distinct from dereferenced PHP values and borrowed element addresses.
//! - Every supported target must emit balanced retain and retirement helpers.

use crate::codegen::platform::Target;
use crate::ir::{Effects, Immediate, LocalKind, Op};
use std::path::Path;

/// Local aliases retain cells, and known object-owned property aliases carry dedicated owner slots.
#[test]
fn reference_alias_owners_are_explicit_on_every_target() {
    let source = r#"<?php
class ReferenceOwner { public array $items = [1]; }
function createReferenceOwners(): void {
    $original = [1, 2];
    $first = &$original;
    $last = &$first;
    unset($original, $first);
    echo $last[0];
    $object = new ReferenceOwner();
    $property = &$object->items;
    echo $property[0];
}
createReferenceOwners();
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let function = module.functions.iter()
            .find(|function| function.name.eq_ignore_ascii_case("createReferenceOwners")).unwrap();
        let retained = function.instructions.iter().filter(|inst| inst.op == Op::RetainLocalRefCell).count();
        assert!(retained >= 2, "{name}: both local aliases retain their cell owner");
        let binding = function.instructions.iter().find(|inst| {
            inst.op == Op::BindRefCellPtr && matches!(inst.immediate, Some(Immediate::LocalSlotPair { .. }))
        }).expect("property alias carries an owned heap cell");
        let Some(Immediate::LocalSlotPair { second: owner, .. }) = binding.immediate else { unreachable!(); };
        assert_eq!(function.locals[owner.as_raw() as usize].kind, LocalKind::RefCell);
        assert_eq!(function.value(binding.operands[0]).unwrap().php_type, crate::types::PhpType::Pointer(None));
        assert!(binding.effects.contains(Effects::REFCOUNT_OP | Effects::WRITES_HEAP));
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(assembly.contains("__rt_incref"), "{name}");
        assert!(assembly.contains("__rt_local_ref_cell_release"), "{name}");
    }
}
