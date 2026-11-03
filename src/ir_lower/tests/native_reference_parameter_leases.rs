//! Purpose:
//! Checks authenticated incoming reference owners without changing native parameter slot order.
//!
//! Called from:
//! - Supported-target EIR and codegen tests in CI.
//!
//! Key details:
//! - Hidden owners follow the complete ABI parameter prefix.
//! - Unset and frame cleanup use the same owner linked to the parameter's physical slot.

use crate::ir::{Immediate, LocalKind, LocalSlotId, Op};

/// A native frame owns each managed incoming cell until detachment or cleanup on every target.
fn verify_native_reference_parameter_leases(target: &str) {
    let source = "<?php function nativeParameterCell(int &$number, string &$text, mixed $value, mixed &...$tail): void { \
        unset($number); echo $text; }";
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let function = module.functions.iter().find(|function| function.name == "nativeParameterCell").unwrap();
    for (index, parameter) in function.params.iter().enumerate() {
        assert_eq!(function.locals[index].name.as_deref(), Some(parameter.name.as_str()),
            "{target}: hidden owners must not displace native parameter slots");
    }
    let leases = function.instructions.iter().filter_map(|inst| {
        if inst.op != Op::RetainLocalRefCell { return None; }
        let Some(Immediate::LocalSlotPair { first, second }) = inst.immediate else { return None; };
        Some((first, second))
    }).collect::<Vec<_>>();
    assert_eq!(leases.len(), 2, "{target}: both reference parameters own a binding");
    assert!(function.params[3].variadic);
    assert_eq!(function.locals[3].ref_cell_owner, None,
        "{target}: a variadic marker array is not one incoming native cell");
    for (index, (source, owner)) in leases.iter().enumerate() {
        assert_eq!(*source, LocalSlotId::from_raw(index as u32));
        assert!(owner.as_raw() as usize >= function.params.len());
        assert_eq!(function.locals[owner.as_raw() as usize].kind, LocalKind::RefCell);
        assert_eq!(function.locals[source.as_raw() as usize].ref_cell_owner, Some(*owner));
    }
    assert!(function.instructions.iter().any(|inst| inst.op == Op::ReleaseLocalRefCell
        && inst.immediate == Some(Immediate::LocalSlot(leases[0].1))), "{target}: unset retires the binding");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    let body = assembly.split_once("@fn name=nativeParameterCell symbol=").unwrap().1
        .split_once("@endfn name=nativeParameterCell").unwrap().0;
    assert!(body.matches("__rt_reference_cell_owner").count() >= 2,
        "{target}: authenticate incoming addresses before retaining managed owners");
    assert!(body.contains("__rt_local_ref_cell_release"), "{target}: frame exit retires remaining owners");
}

supported_target_cases!(native_reference_parameter_leases, verify_native_reference_parameter_leases);
