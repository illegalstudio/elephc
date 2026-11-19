//! Purpose:
//! Emits the reference-safe hash insertion helper used while decoding serialized containers.
//!
//! Called from:
//! - `super::emit_unserialize()` alongside the recursive decoders.
//!
//! Key details:
//! - A duplicate key reuses the existing box instead of letting `__rt_hash_set` release it.
//!   Later `R:`/`r:` slots keep addressing live storage, matching PHP's slot semantics.
//! - The replaced payload is retained in a context-owned box, so its destructor runs after
//!   the parser finishes instead of during a hash update.
//! - Container-valued duplicates become nested Mixed wrappers, keeping the duplicate
//!   value's own reference slot valid without inventing a second storage owner.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::hydration_owners;

use super::property_cleanup::OWNER_CONTEXT;

/// Emits the target-specific insertion and payload-release helpers.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_store_entry_aarch64(emitter),
        Arch::X86_64 => emit_store_entry_x86_64(emitter),
    }
    hydration_owners::emit_release_payload(emitter);
}

/// Preserves the ARM listing's inline assembly annotations.
fn emit_annotated_arm_instruction(emitter: &mut Emitter, instruction: &str, comment: &str) {
    let padding = 76usize.saturating_sub(instruction.len()).max(1);
    let annotation = "//";
    emitter.instruction(&format!("{instruction}{:padding$}{annotation} {comment}", "")); // emit the existing annotated listing
}

/// Emits the AArch64 duplicate-aware entry store.
///
/// Input: x0=hash, x1=key_lo, x2=key_hi (-1 for integer keys), x3=new value box.
/// Output: x0=the possibly reallocated hash.
fn emit_store_entry_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize duplicate-aware entry store ---");
    emitter.label_global("__rt_unser_store_entry");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "x0", 8);
    abi::store_at_offset(emitter, "x1", 16);
    abi::store_at_offset(emitter, "x2", 24);
    abi::store_at_offset(emitter, "x3", 32);

    // -- look up the destination key before mutating the hash --
    abi::load_at_offset(emitter, "x0", 8);
    abi::load_at_offset(emitter, "x1", 16);
    abi::load_at_offset(emitter, "x2", 24);
    abi::emit_call_label(emitter, "__rt_hash_get");
    emit_annotated_arm_instruction(emitter, "cbnz x0, __rt_unser_store_duplicate", "reuse the published box when the key already exists");
    emit_annotated_arm_instruction(emitter, "b __rt_unser_store_insert", "absent keys still take the ordinary insertion path");

    // -- absent key: ordinary insertion transfers the parsed box into the hash --
    emitter.label("__rt_unser_store_insert");
    abi::load_at_offset(emitter, "x0", 8);
    abi::load_at_offset(emitter, "x1", 16);
    abi::load_at_offset(emitter, "x2", 24);
    abi::load_at_offset(emitter, "x3", 32);
    emit_annotated_arm_instruction(emitter, "mov x4, #0", "boxed Mixed values carry no high payload word");
    emit_annotated_arm_instruction(emitter, "mov x5, #7", "value tag = boxed Mixed (transfer the box)");
    abi::emit_call_label(emitter, "__rt_hash_set");
    emit_annotated_arm_instruction(emitter, "b __rt_unser_store_done", "return the possibly grown hash");

    // -- duplicate key: keep the existing box so every published reference stays live --
    emitter.label("__rt_unser_store_duplicate");
    abi::store_at_offset(emitter, "x1", 40);
    hydration_owners::emit_replace_hash_entry(emitter, OWNER_CONTEXT);
    abi::load_at_offset(emitter, "x0", 8);
    emit_annotated_arm_instruction(emitter, "b __rt_unser_store_done", "return the original hash after an in-place duplicate update");

    emitter.label("__rt_unser_store_done");
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

/// Emits the x86_64 duplicate-aware entry store.
///
/// Input: rdi=hash, rsi=key_lo, rdx=key_hi (-1 for integer keys), rcx=new value box.
/// Output: rax=the possibly reallocated hash.
fn emit_store_entry_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize duplicate-aware entry store ---");
    emitter.label_global("__rt_unser_store_entry");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "rdi", 8);
    abi::store_at_offset(emitter, "rsi", 16);
    abi::store_at_offset(emitter, "rdx", 24);
    abi::store_at_offset(emitter, "rcx", 32);

    abi::load_at_offset(emitter, "rdi", 8);
    abi::load_at_offset(emitter, "rsi", 16);
    abi::load_at_offset(emitter, "rdx", 24);
    abi::emit_call_label(emitter, "__rt_hash_get");
    emitter.instruction("test rax, rax");                                       // did the destination key already exist?
    emitter.instruction("jnz __rt_unser_store_duplicate_x");                    // reuse the published box when the key already exists

    emitter.label("__rt_unser_store_insert_x");
    abi::load_at_offset(emitter, "rdi", 8);
    abi::load_at_offset(emitter, "rsi", 16);
    abi::load_at_offset(emitter, "rdx", 24);
    abi::load_at_offset(emitter, "rcx", 32);
    emitter.instruction("mov r8, 0");                                           // boxed Mixed values carry no high payload word
    emitter.instruction("mov r9, 7");                                           // value tag = boxed Mixed (transfer the box)
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.instruction("jmp __rt_unser_store_done_x");                         // return the possibly grown hash

    emitter.label("__rt_unser_store_duplicate_x");
    abi::store_at_offset(emitter, "rdi", 40);
    hydration_owners::emit_replace_hash_entry(emitter, OWNER_CONTEXT);
    abi::load_at_offset(emitter, "rax", 8);                                    // return the original hash after an in-place duplicate update

    emitter.label("__rt_unser_store_done_x");
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target emits the duplicate update before the ordinary insertion and keeps
    /// the replaced payload alive through the parser-context deferral helper.
    #[test]
    fn duplicate_store_contract_holds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_unser_store_entry"), "{name}");
            assert!(asm.contains("__rt_unser_release_payload"), "{name}");
            let duplicate = asm.split_once("__rt_unser_store_duplicate").unwrap().1;
            assert!(duplicate.contains("__rt_mixed_from_value"), "{name}");
            assert!(duplicate.contains("__rt_unserialize_defer_data"), "{name}");
            assert!(duplicate.contains("__rt_unser_release_payload"), "{name}");
            assert!(duplicate.contains("__rt_incref"), "{name}");
            assert!(duplicate.contains("__rt_decref_mixed"), "{name}");
            if name == "linux-x86_64" {
                assert!(duplicate.contains("mov rax, QWORD PTR [rbp - 32]"), "{name}: defer consumes the wrapper in rax");
            }
        }
    }

    /// The ordinary insertion path still delegates to `__rt_hash_set` with a boxed Mixed tag.
    #[test]
    fn absent_key_still_uses_the_shared_hash_insert() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let store = asm.split_once("__rt_unser_store_entry").unwrap().1;
            assert!(store.find("__rt_hash_get").unwrap() < store.find("__rt_hash_set").unwrap(), "{name}");
        }
    }

    /// Every target boxes the retained payload with its own value ABI before deferring it.
    #[test]
    fn replaced_payload_boxing_uses_the_target_abi() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let duplicate = asm.split_once("__rt_unser_store_duplicate").unwrap().1;
            if name == "linux-x86_64" {
                assert!(duplicate.contains("mov rax, QWORD PTR [rbp - 48]"), "{name}");
                assert!(duplicate.contains("mov rdi, QWORD PTR [rbp - 56]"), "{name}");
                assert!(duplicate.contains("mov rsi, QWORD PTR [rbp - 64]"), "{name}");
            } else {
                assert!(duplicate.contains("ldur x0, [x29, #-48]"), "{name}");
                assert!(duplicate.contains("ldur x1, [x29, #-56]"), "{name}");
                assert!(duplicate.contains("ldur x2, [x29, #-64]"), "{name}");
            }
        }
    }
}
