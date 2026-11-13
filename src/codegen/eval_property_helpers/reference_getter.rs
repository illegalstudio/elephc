//! Purpose:
//! Exports persistent aliases of eligible native Mixed instance-property cells to eval.
//!
//! Called from:
//! - Eval property bridge generation and the RuntimeValueOps property-reference adapter.
//!
//! Key details:
//! - Visibility dispatch is shared with ordinary getters; unsupported storage returns no alias.
//! - The returned PHP reference owns its wrapper independently of the receiver and eval scope.

use super::*;

/// Emits a scoped optional reference getter for object-owned Mixed native cells.
pub(super) fn emit(module: &Module, emitter: &mut Emitter, data: &mut DataSection, slots: &[EvalPropertySlot]) {
    let slots = slots.iter().filter(|slot| eligible(module, slot)).cloned().collect::<Vec<_>>();
    let fail = "__elephc_eval_property_reference_fail";
    let done = "__elephc_eval_property_reference_done";
    emitter.blank();
    label_c_global(module, emitter, "__elephc_eval_value_property_reference");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #80");                             // reserve the ordinary scoped-getter frame layout
            emitter.instruction("stp x29, x30, [sp, #64]");                     // preserve the C caller across lookup and promotion
            emitter.instruction("add x29, sp, #64");                            // expose the ordinary getter's frame offsets
            emitter.instruction("str x1, [sp]");                                // retain the requested property-name pointer
            emitter.instruction("str x2, [sp, #8]");                            // retain the requested property-name length
            emitter.instruction("str x0, [sp, #24]");                           // preserve the boxed receiver for visibility probes
            emitter.instruction("str x3, [sp, #32]");                           // retain the active lexical class-scope pointer
            emitter.instruction("str x4, [sp, #40]");                           // retain the active lexical class-scope length
            abi::emit_branch_if_int_result_zero(emitter, fail);
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("cmp x0, #6");                                  // only an object can own native property storage
            emitter.instruction(&format!("b.ne {fail}"));                       // decline unsupported receiver shapes
            emitter.instruction("str x1, [sp, #16]");                           // preserve the physical object for the selected body
            emitter.instruction("ldr x9, [x1]");                                // dispatch by the receiver's actual class identifier
            emit_aarch64_property_dispatch(module, emitter, data, &slots, "reference", fail);
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the C caller and align nested calls
            emitter.instruction("mov rbp, rsp");                                // establish the ordinary scoped-getter frame layout
            emitter.instruction("sub rsp, 48");                                 // reserve property-name, object and lexical-scope spills
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // retain the requested property-name pointer
            emitter.instruction("mov QWORD PTR [rbp - 16], rdx");               // retain the requested property-name length
            emitter.instruction("mov QWORD PTR [rbp - 32], rdi");               // preserve the boxed receiver for visibility probes
            emitter.instruction("mov QWORD PTR [rbp - 40], rcx");               // retain the active lexical class-scope pointer
            emitter.instruction("mov QWORD PTR [rbp - 48], r8");                // retain the active lexical class-scope length
            emitter.instruction("mov rax, rdi");                                // adapt the boxed receiver to the internal runtime ABI
            abi::emit_branch_if_int_result_zero(emitter, fail);
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("cmp rax, 6");                                  // only an object can own native property storage
            emitter.instruction(&format!("jne {fail}"));                        // decline unsupported receiver shapes
            emitter.instruction("mov QWORD PTR [rbp - 24], rdi");               // preserve the physical object for the selected body
            emitter.instruction("mov r11, QWORD PTR [rdi]");                    // dispatch by the receiver's actual class identifier
            emit_x86_64_property_dispatch(module, emitter, data, &slots, "reference", fail);
        }
    }
    abi::emit_jump(emitter, fail);
    for slot in &slots {
        emitter.label(&slot_body_label(module, slot, "reference"));
        let result = abi::int_result_reg(emitter);
        let scratch = abi::secondary_scratch_reg(emitter);
        let marker = abi::tertiary_scratch_reg(emitter);
        let object_offset = if emitter.target.arch == Arch::AArch64 { 48 } else { 24 };
        abi::load_at_offset(emitter, result, object_offset);
        abi::emit_load_from_address(emitter, result, result, slot.offset);
        abi::emit_load_from_address(emitter, scratch, result, 0);
        abi::emit_load_int_immediate(emitter, marker, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {scratch}, {marker}"));       // preserve uninitialized typed-property behavior
                emitter.instruction(&format!("b.eq {fail}"));                   // do not install an alias over the property sentinel
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {scratch}, {marker}"));       // preserve uninitialized typed-property behavior
                emitter.instruction(&format!("je {fail}"));                     // do not install an alias over the property sentinel
            }
        }
        abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
        abi::emit_load_from_address(emitter, result, result, 0);
        abi::emit_incref_if_refcounted(emitter, &PhpType::Mixed);
        abi::emit_jump(emitter, done);
    }
    emitter.label(fail);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #64]");                     // restore the C caller's frame and return address
            emitter.instruction("add sp, sp, #80");                             // discard the scoped-getter spills
        }
        Arch::X86_64 => { emitter.instruction("leave"); }                       // discard spills and restore the C caller's frame pointer
    }
    abi::emit_return(emitter);
}

/// Limits the first canonical-storage layer to owned user Mixed slots with ordinary backing.
fn eligible(module: &Module, slot: &EvalPropertySlot) -> bool {
    slot.is_reference && slot.ty.codegen_repr() == PhpType::Mixed
        && module.class_infos.get(&slot.declaring_class).is_some_and(|class|
            !class.is_internal && class.property_slot_owns_reference((slot.offset - 8) / 16)
                && class.properties.get((slot.offset - 8) / 16).is_some_and(|(_, ty)| ty == &PhpType::Mixed)
                && !class.readonly_properties.contains(&slot.property)
                && !class.property_set_visibilities.contains_key(&slot.property)
                && class.property_hooks.get(&slot.property).is_none_or(|hooks| !hooks.any()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Mixed constructor slots own exportable cells while raw and constrained storage stays distinct.
    #[test]
    fn native_mixed_reference_export_preserves_storage_boundaries_on_every_target() {
        let source = "<?php class MixedExportParent { public mixed $value = 'old'; \
            public int $number = 1; public int|string $union = 1; } \
            class MixedExportChild extends MixedExportParent {} \
            class BorrowedExport { public function __construct(public mixed &$borrowed) {} } \
            class RawBorrowedExport { public function __construct(public int &$borrowed) {} } \
            $code = $argc > 0 ? '' : 'echo 1;'; eval($code);";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            for class in ["MixedExportParent", "MixedExportChild", "BorrowedExport", "RawBorrowedExport"] {
                module.class_infos.insert(class.to_string(), checked.classes[class].clone());
            }
            let slots = collect_eval_property_slots(&module);
            for class in ["MixedExportParent", "MixedExportChild"] {
                let info = &module.class_infos[class];
                assert!(info.owned_reference_properties.contains("value"), "{name}: owned Mixed backing");
                assert!(!info.reference_properties.contains("number"), "{name}: raw integer layout");
                assert!(!info.reference_properties.contains("union"), "{name}: constrained union layout");
            }
            assert!(module.class_infos["BorrowedExport"].owned_reference_properties.contains("borrowed"));
            assert!(!module.class_infos["RawBorrowedExport"].owned_reference_properties.contains("borrowed"));
            assert_eq!(slots.iter().filter(|slot| eligible(&module, slot)).count(), 3);
            let mut emitter = Emitter::new(target);
            emit(&module, &mut emitter, &mut DataSection::new(), &slots);
            let assembly = emitter.output();
            assert_eq!(assembly.matches("__rt_reference_cell_promote_mixed").count(), 3);
            assert!(!assembly.contains("__rt_mixed_clone"), "{name}: reference export must preserve its current child");
            assert!(assembly.contains("__rt_incref"), "{name}: the exported wrapper owns a receiver-independent lease");
            assert!(!assembly.contains("RawBorrowedExport"), "{name}: raw borrowed constructor cells are not promoted");
        }
    }
    /// Private ancestor cells retain physical ownership beneath both promoted and raw shadows.
    fn verify_hidden_mixed_reference_cells_keep_physical_ownership(name: &str) {
        let source = "<?php class HiddenCellParent { private mixed $value = 'parent'; } \
            class HiddenCellChild extends HiddenCellParent { public mixed $value = 'child'; } \
            class HiddenRawChild extends HiddenCellParent { public string $value = 'raw'; } \
            $code = $argc > 0 ? '' : 'echo 1;'; eval($code);";
        let target = Target::parse(name).unwrap();
        let module = crate::ir_lower::tests::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        for (class_name, owned) in [("HiddenCellParent", vec![0]),
            ("HiddenCellChild", vec![0, 1]), ("HiddenRawChild", vec![0])] {
            let class = &module.class_infos[class_name];
            assert_eq!(class.owned_reference_property_slots, owned.into_iter().collect(), "{name}: {class_name}");
            assert!(class.property_slot_is_reference(0, "value"), "{name}: hidden ancestor cell");
        }
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let raw = &module.class_infos["HiddenRawChild"];
        let marker = format!("_class_gc_desc_{}:\n", raw.class_id);
        assert!(assembly.split_once(&marker).unwrap().1.starts_with("    .byte 11, 1\n"),
            "{name}: hidden cell and raw string keep separate destruction contracts");
    }

    /// Schedules the fixture once per supported target.
    mod hidden_mixed_reference_cells_keep_physical_ownership_on_every_target {
        /// Verifies the fixture on macos-aarch64.
        #[test]
        fn macos_aarch64() { super::verify_hidden_mixed_reference_cells_keep_physical_ownership("macos-aarch64"); }
        /// Verifies the fixture on ios-arm64.
        #[test]
        fn ios_arm64() { super::verify_hidden_mixed_reference_cells_keep_physical_ownership("ios-arm64"); }
        /// Verifies the fixture on ios-sim-arm64.
        #[test]
        fn ios_sim_arm64() { super::verify_hidden_mixed_reference_cells_keep_physical_ownership("ios-sim-arm64"); }
        /// Verifies the fixture on linux-aarch64.
        #[test]
        fn linux_aarch64() { super::verify_hidden_mixed_reference_cells_keep_physical_ownership("linux-aarch64"); }
        /// Verifies the fixture on linux-x86_64.
        #[test]
        fn linux_x86_64() { super::verify_hidden_mixed_reference_cells_keep_physical_ownership("linux-x86_64"); }
    }

}
