//! Purpose:
//! Regression coverage for declared PHP arrays lowered into two-array `array_multisort`.
//!
//! Called from:
//! - `crate::ir_lower::tests` through Rust's test harness.
//!
//! Key details:
//! - Declared array references remain boxed Mixed operands in EIR.
//! - Every supported target must select two cell COW operations and the boxed tandem sorter.

use crate::ir::{Immediate, RuntimeCallTarget, RuntimeFnId};

/// Declared property arguments are rewritten to writable locals before backend COW selection.
#[test]
fn multisort_declared_properties_reach_lowered_receiver_slots_on_every_target() {
    let source = r#"<?php
class MultisortPropertySlots { public array $values = [3, 1, 2]; }
function sortPropertySlots(MultisortPropertySlots $holder): void {
    array_multisort($holder->values, $holder->values);
}
sortPropertySlots(new MultisortPropertySlots());
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "sortPropertySlots").unwrap();
        let call = function.instructions.iter().find(|inst| matches!(inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayMultisort)))
            | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::ArrayMultisort, .. }))
        )).unwrap();
        assert_eq!(call.operands.len(), 2, "{name}");
        for value in &call.operands {
            let load = function.instructions.iter().find(|inst| inst.result == Some(*value)).unwrap();
            assert_eq!(load.op, crate::ir::Op::LoadLocal, "{name}: property mutation passes a stabilized local");
            assert!(matches!(load.immediate, Some(Immediate::LocalSlot(_))), "{name}: backend resolves a writable slot");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

/// Same-place concrete operands can each own a payload detached from final Mixed storage.
#[test]
fn multisort_same_widened_local_retires_the_second_detached_payload_on_every_target() {
    let source = r#"<?php
function sortWidenedTwice(string $source): void {
    eval($source);
    $values = [3, 1, 2];
    array_multisort($values, $values);
    echo implode(",", $values);
}
sortWidenedTwice('return null; // ' . $argc);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "sortWidenedTwice").unwrap();
        let call = function.instructions.iter().find(|inst| matches!(inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::ArrayMultisort)))
            | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::ArrayMultisort, .. }))
        )).unwrap();
        assert_ne!(call.operands[0], call.operands[1], "{name}: two independent local reads");
        let mut slots = Vec::new();
        for value in &call.operands {
            let load = function.instructions.iter().find(|inst| inst.result == Some(*value)).unwrap();
            assert_eq!(load.op, crate::ir::Op::LoadLocal, "{name}");
            assert!(matches!(load.result_php_type, crate::types::PhpType::Array(_)), "{name}");
            let Some(Immediate::LocalSlot(slot)) = load.immediate else { panic!("receiver slot"); };
            assert_eq!(function.locals[slot.as_raw() as usize].php_type.codegen_repr(), crate::types::PhpType::Mixed);
            slots.push(slot);
            assert!(!function.instructions.iter().any(|inst| {
                inst.op == crate::ir::Op::Release && inst.operands == [*value]
            }), "{name}: mutation transfers the detached lease instead of post-call releasing it");
        }
        assert_eq!(slots[0], slots[1], "{name}: both reads name the same widened local");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let same_place = assembly.split("array_multisort_distinct_receivers").nth(1).unwrap();
        let same_place = same_place.split("array_multisort_receivers_ready").next().unwrap();
        assert!(same_place.contains("__rt_decref_any"), "{name}: retire the second detached payload before cache replacement");
    }
}

/// Declared by-reference arrays reach the boxed multisort backend on every supported target.
#[test]
fn declared_array_multisort_uses_boxed_cow_and_tuple_comparison_on_every_target() {
    let source = r#"<?php
function sortRows(array &$primary, array &$secondary): bool {
    return array_multisort($primary, $secondary);
}
$primary = [2, 1, 2];
$secondary = ["z", "m", "a"];
sortRows($primary, $secondary);
"#;
    for name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(name).unwrap(),
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "sortRows")
            .expect("declared multisort wrapper");
        let calls = function
            .instructions
            .iter()
            .filter(|inst| {
                matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                        RuntimeFnId::ArrayMultisort,
                    )))
                        | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                            target: RuntimeFnId::ArrayMultisort,
                            ..
                        }))
                )
            })
            .count();
        assert_eq!(calls, 1, "{name}");

        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(
            assembly.matches("__rt_array_cell_ensure_unique").count(),
            2,
            "{name}"
        );
        assert_eq!(
            assembly.matches("__rt_mixed_sort_require_scalars").count(),
            2,
            "{name}"
        );
        assert!(assembly.contains("__rt_array_multisort_boxed"), "{name}");
        assert_eq!(assembly.matches("__rt_array_ensure_unique").count(), 2, "{name}: normalize payloads independently");
        assert!(assembly.contains("array_multisort_distinct_receivers"), "{name}: compare lvalues before COW");
        assert!(assembly.contains("_spl_value_error_class_id"), "{name}");
    }
}
