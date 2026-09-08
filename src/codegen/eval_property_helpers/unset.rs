//! Purpose:
//! Restores native typed instance properties to their uninitialized state from eval.
//!
//! Called from:
//! - The eval property helper emitter and Magician's authorized property-unset path.
//!
//! Key details:
//! - Uses the getter frame layout so visibility and private-shadow dispatch stay shared.
//! - Clears storage before releasing its previous owner, including reentrant destructors.
//! - Returns false for non-native slots so eval can update its separate property storage.

use super::*;

/// Emits the native typed-property unset entry point for every supported target.
pub(super) fn emit_property_unset_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    super::unset_boundary::emit(module, emitter);
    let slots = slots.iter().filter(|slot| slot.is_declared).cloned().collect::<Vec<_>>();
    let fail = "__elephc_eval_value_typed_property_unset_miss";
    let done = "__elephc_eval_value_typed_property_unset_done";
    emitter.blank();
    emitter.label_global("__rt_eval_typed_property_unset");
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #80");                             // reserve the shared getter-layout frame
            emitter.instruction("stp x29, x30, [sp, #64]");                     // preserve the caller frame across visibility and release helpers
            emitter.instruction("add x29, sp, #64");                            // establish the helper frame pointer
            emitter.instruction("str x1, [sp]");                                // preserve the requested property name
            emitter.instruction("str x2, [sp, #8]");                            // preserve its byte length
            emitter.instruction("str x3, [sp, #32]");                           // preserve the authorized class scope
            emitter.instruction("str x4, [sp, #40]");                           // preserve the scope length
            emitter.instruction(&format!("cbz x0, {fail}"));                    // missing receivers have no native slot
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("cmp x0, #6");                                  // require an object payload before accessing its layout
            emitter.instruction(&format!("b.ne {fail}"));                       // reject non-object receivers
            emitter.instruction("str x1, [sp, #16]");                           // root the raw receiver in the shared property-dispatch spill
            emitter.instruction("ldr x9, [x1]");                                // select slots using the physical native class id
            emit_aarch64_property_dispatch(module, emitter, data, &slots, "unset", fail);
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the Rust caller frame
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame pointer
            emitter.instruction("sub rsp, 48");                                 // reserve the aligned shared getter-layout frame
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // preserve the requested property name
            emitter.instruction("mov QWORD PTR [rbp - 16], rdx");               // preserve its byte length
            emitter.instruction("mov QWORD PTR [rbp - 40], rcx");               // preserve the authorized class scope
            emitter.instruction("mov QWORD PTR [rbp - 48], r8");                // preserve the scope length
            emitter.instruction("test rdi, rdi");                               // reject a missing receiver before unboxing
            emitter.instruction(&format!("jz {fail}"));                         // missing receivers have no native slot
            emitter.instruction("mov rax, rdi");                                // pass the boxed receiver through the internal runtime ABI
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("cmp rax, 6");                                  // require an object payload before accessing its layout
            emitter.instruction(&format!("jne {fail}"));                        // reject non-object receivers
            emitter.instruction("mov QWORD PTR [rbp - 24], rdi");               // preserve the receiver in the shared property-dispatch spill
            emitter.instruction("mov r11, QWORD PTR [rdi]");                    // select slots using the physical native class id
            emit_x86_64_property_dispatch(module, emitter, data, &slots, "unset", fail);
        }
    }
    abi::emit_jump(emitter, fail);
    for slot in &slots {
        emitter.label(&slot_body_label(module, slot, "unset"));
        emit_unset_slot(emitter, slot);
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
        abi::emit_jump(emitter, done);
    }
    emitter.label(fail);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    emitter.label(done);
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #64]");                     // restore the caller frame and return address
            emitter.instruction("add sp, sp, #80");                             // release the helper frame
        }
        Arch::X86_64 => {
            emitter.instruction("leave");                                       // restore the caller stack and frame pointer
        }
    }
    abi::emit_return(emitter);
}

/// Detaches one typed slot's payload before releasing its old heap owner.
fn emit_unset_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    let (receiver, marker, payload) = match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #16]");                           // recover the live receiver selected by property dispatch
            ("x9", "x10", "x0")
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // recover the live receiver selected by property dispatch
            ("r11", "r10", "rax")
        }
    };
    abi::emit_load_from_address(emitter, payload, receiver, slot.offset);
    abi::emit_load_int_immediate(emitter, marker, 0);
    abi::emit_store_to_address(emitter, marker, receiver, slot.offset);
    abi::emit_load_int_immediate(emitter, marker, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    abi::emit_store_to_address(emitter, marker, receiver, slot.offset + 8);
    if slot.ty == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_decref_any");
    } else {
        abi::emit_decref_if_refcounted(emitter, &slot.ty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Each target emits the native marker update before releasing an object property payload.
    #[test]
    fn typed_unset_marks_storage_before_releasing_its_owner() {
        let tokens = crate::lexer::tokenize("<?php class UnsetStorage { public object $value; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            module.class_infos.insert("UnsetStorage".into(), checked.classes["UnsetStorage"].clone());
            let slots = collect_eval_property_slots(&module);
            let mut emitter = Emitter::new(target);
            emit_property_unset_helper(&module, &mut emitter, &mut DataSection::new(), &slots);
            let asm = emitter.output();
            assert!(asm.contains(&target.extern_symbol("__elephc_eval_value_typed_property_unset_v2")), "{name}");
            let release = asm.find("__rt_decref_object").unwrap();
            let store = if target.arch == Arch::AArch64 { "str x10, [x9, #16]" } else { "mov QWORD PTR [r11 + 16], r10" };
            assert!(asm.find(store).is_some_and(|marker| marker < release), "{name}");
        }
    }
}
