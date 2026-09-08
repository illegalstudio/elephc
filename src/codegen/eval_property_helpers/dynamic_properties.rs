//! Purpose:
//! Routes eval-only subclass fields to GC-visible property hashes on native objects.
//!
//! Called from:
//! - The eval property bridge after declared-property dispatch misses.
//!
//! Key details:
//! - Extra storage does not grant ordinary native objects PHP dynamic-property permission.
//! - Declared slots and their visibility gates always take precedence over this fallback.
//! - Hash setters consume a retained cell owner and publish COW or growth replacements.

use super::{abi, Arch, Emitter, Module};

const SLOT_HELPER: &str = "__elephc_eval_property_hash_slot";

/// Returns a permitted object's property-hash slot address, or zero without touching its fields.
pub(super) fn emit_property_hash_slot_helper(module: &Module, emitter: &mut Emitter) {
    emitter.label(SLOT_HELPER);
    let miss = "__elephc_eval_property_hash_slot_miss";
    let done = "__elephc_eval_property_hash_slot_done";
    let (class_reg, compare_reg, object_reg, result_reg) = match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // preserve the raw receiver across the ownership callback
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save the caller frame and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame
            emitter.instruction("str x0, [sp]");                                // keep the native receiver live without transferring ownership
            emitter.instruction("ldr x9, [x0]");                                // read its physical native class id
            ("x9", "x10", "x0", "x0")
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 16");                                 // reserve an aligned receiver spill
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // preserve the raw object across the callback
            emitter.instruction("mov r11, QWORD PTR [rdi]");                    // read its physical native class id
            ("r11", "r10", "rdi", "rax")
        }
    };
    let mut classes = module.class_infos.iter().filter(|(name, info)| {
        name.as_str() == "stdClass" || info.has_property_hash_storage()
    }).collect::<Vec<_>>();
    classes.sort_by_key(|(_, info)| info.class_id);
    for (name, info) in classes {
        let next = format!("{SLOT_HELPER}_next_{}", info.class_id);
        abi::emit_load_int_immediate(emitter, compare_reg, info.class_id as i64);
        match module.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {class_reg}, {compare_reg}")); // match a known layout before computing an offset
                emitter.instruction(&format!("b.ne {next}"));                   // try the next layout on a class mismatch
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {class_reg}, {compare_reg}")); // match a known layout before computing an offset
                emitter.instruction(&format!("jne {next}"));                    // try the next layout on a class mismatch
            }
        }
        if name != "stdClass" && !info.allow_dynamic_properties {
            let callback = module.target.extern_symbol("__elephc_eval_dynamic_object_owns_properties");
            abi::emit_call_label(emitter, &callback);
            match module.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("cbz x0, {miss}"));            // only eval-declared instances may use the extra storage
                    emitter.instruction("ldr x0, [sp]");                        // recover the receiver after the ownership check
                }
                Arch::X86_64 => {
                    emitter.instruction("test rax, rax");                       // inspect eval ownership without changing PHP attributes
                    emitter.instruction(&format!("jz {miss}"));                 // deny ordinary native objects
                    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");        // recover the receiver after the callback
                }
            }
        }
        let offset = 8 + info.properties.len() * 16;
        match module.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(emitter, "x10", offset as i64);
                emitter.instruction(&format!("add {result_reg}, {object_reg}, x10")); // return the physical tail slot, not its current hash value
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("lea {result_reg}, [{object_reg} + {offset}]")); // return the physical tail slot for reads or COW writes
            }
        }
        abi::emit_jump(emitter, done);
        emitter.label(&next);
    }
    emitter.label(miss);
    abi::emit_load_int_immediate(emitter, result_reg, 0);
    emitter.label(done);
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore the caller frame and return address
            emitter.instruction("add sp, sp, #32");                             // discard the receiver spill
        }
        Arch::X86_64 => {
            emitter.instruction("leave");                                       // restore the caller's stack and frame pointer
        }
    }
    abi::emit_return(emitter);
}

/// Reads a fallback hash using the enclosing property getter's saved receiver and key.
pub(super) fn emit_dynamic_property_get_fallback(emitter: &mut Emitter, null_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #16]");                           // recover the native object after declared-property dispatch
            abi::emit_call_label(emitter, SLOT_HELPER);
            emitter.instruction(&format!("cbz x0, {null_label}"));              // preserve null reads when no fallback is permitted
            emitter.instruction("ldr x1, [sp]");                                // recover the requested property-name pointer
            emitter.instruction("ldr x2, [sp, #8]");                            // recover its byte length
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // recover the native object after declared-property dispatch
            abi::emit_call_label(emitter, SLOT_HELPER);
            emitter.instruction("test rax, rax");                               // check whether fallback storage is available
            emitter.instruction(&format!("jz {null_label}"));                   // preserve null reads when no fallback is permitted
            emitter.instruction("mov rdi, rax");                                // pass the hash-slot address through the property helper ABI
            emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                // recover the requested property-name pointer
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");               // recover its byte length
        }
    }
    abi::emit_call_label(emitter, "__rt_property_hash_get");
}

/// Writes a fallback hash with an independent retained owner for the borrowed input cell.
pub(super) fn emit_dynamic_property_set_fallback(emitter: &mut Emitter, fail: &str, done: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #16]");                           // recover the native receiver after declared-property dispatch
            abi::emit_call_label(emitter, SLOT_HELPER);
            emitter.instruction(&format!("cbz x0, {fail}"));                    // reject writes without a permitted property hash
            emitter.instruction("str x0, [sp, #32]");                           // reuse the no-longer-needed boxed receiver slot
            emitter.instruction("ldr x0, [sp, #24]");                           // acquire a hash owner for the borrowed value
            abi::emit_call_label(emitter, "__rt_incref");
            emitter.instruction("ldr x0, [sp, #32]");                           // pass the owning hash-slot address
            emitter.instruction("ldr x1, [sp]");                                // recover the requested property-name pointer
            emitter.instruction("ldr x2, [sp, #8]");                            // recover its byte length
            emitter.instruction("ldr x3, [sp, #24]");                           // transfer the acquired boxed-cell owner into storage
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // recover the native receiver after declared-property dispatch
            abi::emit_call_label(emitter, SLOT_HELPER);
            emitter.instruction("test rax, rax");                               // inspect the permitted hash-slot address
            emitter.instruction(&format!("jz {fail}"));                         // reject writes without fallback storage
            emitter.instruction("mov QWORD PTR [rbp - 40], rax");               // reuse the no-longer-needed boxed receiver slot
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // acquire a hash owner for the borrowed value
            abi::emit_call_label(emitter, "__rt_incref");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");               // pass the owning hash-slot address
            emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                // recover the requested property-name pointer
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");               // recover its byte length
            emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");               // transfer the acquired boxed-cell owner into storage
        }
    }
    abi::emit_call_label(emitter, "__rt_property_hash_set");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    abi::emit_jump(emitter, done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target guards extra native storage with the correctly mangled eval ownership callback.
    #[test]
    fn eval_subclass_hash_guard_uses_target_abi_without_granting_dynamic_properties() {
        let tokens = crate::lexer::tokenize("<?php class StoredParent { public int $x = 1; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = Module::new(target);
            let mut info = checked.classes["StoredParent"].clone();
            info.eval_property_storage = true;
            assert!(!info.allow_dynamic_properties);
            module.class_infos.insert("StoredParent".to_string(), info);
            let mut emitter = Emitter::new(target);
            emit_property_hash_slot_helper(&module, &mut emitter);
            let asm = emitter.output();
            let callback = target.extern_symbol("__elephc_eval_dynamic_object_owns_properties");
            let call = if target.arch == Arch::AArch64 { "bl" } else { "call" };
            assert!(asm.contains(&format!("{call} {callback}")), "{name}");
            assert!(asm.contains("__elephc_eval_property_hash_slot_miss:"), "{name}");
            let offset = if target.arch == Arch::AArch64 { "#24" } else { "[rdi + 24]" };
            assert!(asm.contains(offset), "{name}: missing one-property tail offset");
        }
    }
}
