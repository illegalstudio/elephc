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
    assert!(body.matches("__rt_reference_parameter_acquire").count() >= 2,
        "{target}: authenticate and claim transferred owners, retaining for other call surfaces");
    assert!(body.contains("__rt_local_ref_cell_release"), "{target}: frame exit retires remaining owners");
}

supported_target_cases!(native_reference_parameter_leases, verify_native_reference_parameter_leases);

/// Eager generator guards cannot consume the source or staging owners before capture.
fn verify_generator_reference_parameter_guard_borrows(target: &str) {
    let source = "<?php class GuardedReferenceProbe {} \
        function guardedReferenceGenerator(mixed &$value, GuardedReferenceProbe $object): Generator { \
            yield $value; }";
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let validator = module.closures.iter()
        .find(|function| function.flags.is_eager_parameter_validator).unwrap();
    assert!(!validator.flags.is_generator, "{target}: the guard is an eager ordinary function");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let body = assembly.split_once(&format!("@fn name={} symbol=", validator.name)).unwrap().1
        .split_once(&format!("@endfn name={}", validator.name)).unwrap().0;
    assert!(body.contains("__rt_reference_cell_owner") && body.contains("__rt_incref"),
        "{target}: borrow authenticated storage while validating parameters");
    assert!(!body.contains("__rt_reference_parameter_acquire"),
        "{target}: only the real native invocation can claim argument owners");
}

supported_target_cases!(generator_reference_parameter_guard_borrows, verify_generator_reference_parameter_guard_borrows);


/// Direct property arguments keep their physical cell and prepublish a transferable caller owner.
fn verify_native_property_argument_handoff(target: &str) {
    let source = r#"<?php
class NativeArgumentLeaseProbe { public mixed $value = 'before'; }
function nativeArgumentLeaseTarget(mixed &$value, NativeArgumentLeaseProbe $copy): void {
    $value = 'written'; echo $copy->value;
}
$object = new NativeArgumentLeaseProbe();
nativeArgumentLeaseTarget($object->value, clone $object);
echo $object->value;
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    assert!(module.class_infos["NativeArgumentLeaseProbe"].property_slot_owns_reference(0),
        "{target}: the checker must choose stable physical property storage before EIR");
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    let owners = main.locals.iter().filter(|local| local.is_reference_argument_owner).collect::<Vec<_>>();
    assert_eq!(owners.len(), 1, "{target}: exactly one pending PHP reference argument");
    assert_eq!(owners[0].kind, LocalKind::RefCell);
    let publish = main.instructions.iter().position(|inst| inst.op == Op::PushCallOperandOwner
        && inst.immediate == Some(Immediate::LocalSlot(owners[0].id))).unwrap();
    let reference = main.instructions.iter().position(|inst| inst.op == Op::LoadPropRefCell).unwrap();
    assert!(publish < reference, "{target}: receiver cleanup cannot throw before the owner guard is published");
    assert!(!main.instructions.iter().any(|inst| inst.op == Op::PropSet),
        "{target}: a live reference must not be detached and copied back after the call");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    assert!(assembly.contains("__rt_cleanup_call_reference_argument"), "{target}: a distinct transfer-capable record");
    assert!(assembly.contains("reference_argument_activate_found"), "{target}: activate at the final native call SP");
    let key = if target == "linux-x86_64" { "mov QWORD PTR [r10 + 32], rsp" }
        else { "str x9, [x10, #32]" };
    assert!(assembly.contains(key), "{target}: native entry identity preserves pending outer calls");
}

supported_target_cases!(native_property_argument_handoff, verify_native_property_argument_handoff);
