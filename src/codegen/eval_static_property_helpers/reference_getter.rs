//! Purpose:
//! Exports native Mixed static properties as independently retained persistent PHP references.
//!
//! Called from:
//! - Eval static-property bridge generation and the optional runtime reference callback.
//!
//! Key details:
//! - Scoped dispatch matches ordinary static reads; initialization is checked before promotion.
//! - The stable symbol owns one wrapper until static teardown, independently of escaped aliases.

use super::*;

/// Emits the optional reference callback for initialized user-declared Mixed static slots.
pub(super) fn emit(module: &Module, emitter: &mut Emitter, data: &mut DataSection, slots: &[EvalStaticPropertySlot]) {
    let slots = slots.iter().filter(|slot| {
        slot.is_declared && module.class_infos.get(&slot.declaring_class).is_some_and(|class|
            !class.is_internal && class.static_properties.iter()
                .any(|(name, ty)| name == &slot.property && ty == &PhpType::Mixed))
    }).cloned().collect::<Vec<_>>();
    emitter.blank();
    label_c_global(module, emitter, "__elephc_eval_value_static_property_reference");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(module, emitter, data, &slots),
        Arch::X86_64 => emit_x86_64(module, emitter, data, &slots),
    }
}

/// Emits the scoped static reference lookup with the ordinary getter ABI.
fn emit_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let done_label = "__elephc_eval_value_static_property_reference_done";
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for class/property/scope slices and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested property-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested property-name length
    emitter.instruction("str x4, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x5, [sp, #40]");                                   // save the active eval class-scope length
    emit_aarch64_static_property_dispatch(module, emitter, data, slots, "reference");
    emitter.instruction("mov x0, xzr");                                         // report bridge miss with a null pointer
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after a miss
    emit_slot_bodies(module, emitter, slots, done_label);
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed static property value to Rust
}

/// Emits the scoped static reference lookup with the ordinary getter ABI.
fn emit_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let done_label = "__elephc_eval_value_static_property_reference_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for class, property, and scope slices
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the active eval class-scope length
    emit_x86_64_static_property_dispatch(module, emitter, data, slots, "reference");
    emitter.instruction("xor eax, eax");                                        // report bridge miss with a null pointer
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after a miss
    emit_slot_bodies(module, emitter, slots, done_label);
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed static property value to Rust
}

/// Promotes a symbol's existing value without moving its native address or retaining a receiver.
fn emit_slot_bodies(module: &Module, emitter: &mut Emitter, slots: &[EvalStaticPropertySlot], done: &str) {
    for slot in slots {
        let label = slot_body_label(module, slot, "reference");
        emitter.label(&label);
        let initialized = format!("{label}_initialized");
        let result = abi::int_result_reg(emitter);
        let marker = abi::secondary_scratch_reg(emitter);
        let sentinel = abi::tertiary_scratch_reg(emitter);
        abi::emit_load_symbol_to_reg(emitter, marker, &slot.symbol, 8);
        abi::emit_load_int_immediate(emitter, sentinel, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {marker}, {sentinel}"));      // inspect the static initialization word before reading its value
                emitter.instruction(&format!("b.ne {initialized}"));            // initialized storage can publish a persistent reference
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {marker}, {sentinel}"));      // inspect the static initialization word before reading its value
                emitter.instruction(&format!("jne {initialized}"));             // initialized storage can publish a persistent reference
            }
        }
        abi::emit_load_int_immediate(emitter, result, 0);
        abi::emit_jump(emitter, done);
        emitter.label(&initialized);
        abi::emit_symbol_address(emitter, result, &slot.symbol);
        abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
        abi::emit_load_from_address(emitter, result, result, 0);
        abi::emit_call_label(emitter, "__rt_incref");
        abi::emit_jump(emitter, done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Static exports preserve scoped lookup and retain wrappers without copying their values.
    #[test]
    fn mixed_static_reference_exports_on_every_target() {
        let source = "<?php class StaticReferenceBase { public static mixed $value = 1; \
            protected static mixed $secret = 'hidden'; public static int $raw = 1; \
            public static int|string $union = 1; } class StaticReferenceChild extends StaticReferenceBase {}";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            for class_name in ["StaticReferenceBase", "StaticReferenceChild"] {
                module.class_infos.insert(class_name.to_string(), checked.classes[class_name].clone());
            }
            let slots = collect_eval_static_property_slots(&module);
            let mut emitter = Emitter::new(target);
            emit(&module, &mut emitter, &mut DataSection::new(), &slots);
            let assembly = emitter.output();
            assert_eq!(assembly.matches("__rt_reference_cell_promote_mixed").count(), 4, "{name}");
            assert!(assembly.contains("__rt_incref"), "{name}: an alias outlives the static symbol's owner");
            assert!(!assembly.contains("__rt_mixed_clone"), "{name}: exporting aliases must preserve identity");
            assert!(!assembly.contains(&static_property_symbol("StaticReferenceBase", "raw")), "{name}: raw typed storage stays distinct");
            assert!(!assembly.contains(&static_property_symbol("StaticReferenceBase", "union")), "{name}: union activation requires its constraint ledger");
        }
    }

    /// Eval reads resolve the optional cell layout without changing the ordinary raw getter ABI.
    #[test]
    fn mixed_static_eval_readers_resolve_managed_bindings_on_every_target() {
        let source = "<?php class EvalStaticCellRead { public static mixed $value = 41; }";
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for native_validation in [false, true] {
                let mut module = Module::new(target);
                module.required_runtime_features.native_reference_validation = native_validation;
                module.class_infos.insert("EvalStaticCellRead".into(), checked.classes["EvalStaticCellRead"].clone());
                let slots = collect_eval_static_property_slots(&module);
                let mut emitter = Emitter::new(target);
                emit_static_property_get_helper(&module, &mut emitter, &mut DataSection::new(), &slots);
                let assembly = emitter.output();
                assert_eq!(assembly.contains("__rt_reference_static_mixed_payload"), native_validation,
                    "{name}: optional layout selection must match the linked runtime");
                assert!(assembly.contains("__rt_mixed_deref"), "{name}: expose the wrapper's current child");
                assert!(assembly.contains("__rt_incref"), "{name}: eval owns the returned value independently");
            }
        }
    }

}
