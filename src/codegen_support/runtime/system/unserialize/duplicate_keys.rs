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

/// Emits the target-specific insertion and payload-release helpers.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_store_entry_aarch64(emitter),
        Arch::X86_64 => emit_store_entry_x86_64(emitter),
    }
    match emitter.target.arch {
        Arch::AArch64 => emit_release_payload_aarch64(emitter),
        Arch::X86_64 => emit_release_payload_x86_64(emitter),
    }
}

/// Emits one instruction with the repository's column-81 comment alignment.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str, comment: &str) {
    let text = if emitter.target.arch == Arch::AArch64 { arm } else { x86 };
    let padding = 76usize.saturating_sub(text.len()).max(1);
    emitter.instruction(&format!("{text}{}                                      // {comment}", " ".repeat(padding)));
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
    ins(emitter, "cbnz x0, __rt_unser_store_duplicate", "test rax, rax", "reuse the published box when the key already exists");
    ins(emitter, "b __rt_unser_store_insert", "jz __rt_unser_store_insert_x", "absent keys still take the ordinary insertion path");

    // -- absent key: ordinary insertion transfers the parsed box into the hash --
    emitter.label("__rt_unser_store_insert");
    abi::load_at_offset(emitter, "x0", 8);
    abi::load_at_offset(emitter, "x1", 16);
    abi::load_at_offset(emitter, "x2", 24);
    abi::load_at_offset(emitter, "x3", 32);
    ins(emitter, "mov x4, #0", "mov r8, 0", "boxed Mixed values carry no high payload word");
    ins(emitter, "mov x5, #7", "mov r9, 7", "value tag = boxed Mixed (transfer the box)");
    abi::emit_call_label(emitter, "__rt_hash_set");
    ins(emitter, "b __rt_unser_store_done", "jmp __rt_unser_store_done_x", "return the possibly grown hash");

    // -- duplicate key: keep the existing box so every published reference stays live --
    emitter.label("__rt_unser_store_duplicate");
    abi::store_at_offset(emitter, "x1", 40);
    abi::load_at_offset(emitter, "x9", 40);
    ins(emitter, "ldr x10, [x9]", "mov r10, QWORD PTR [r9]", "previous value tag");
    abi::store_at_offset(emitter, "x10", 48);
    ins(emitter, "ldr x10, [x9, #8]", "mov r10, QWORD PTR [r9 + 8]", "previous payload low word");
    abi::store_at_offset(emitter, "x10", 56);
    ins(emitter, "ldr x10, [x9, #16]", "mov r10, QWORD PTR [r9 + 16]", "previous payload high word");
    abi::store_at_offset(emitter, "x10", 64);

    // -- retain the replaced payload until the parser context completes --
    abi::load_at_offset(emitter, "x0", 48);
    abi::load_at_offset(emitter, "x1", 56);
    abi::load_at_offset(emitter, "x2", 64);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, "x0", 72);
    abi::load_at_offset(emitter, "x0", 72);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    abi::load_at_offset(emitter, "x0", 48);
    abi::load_at_offset(emitter, "x1", 56);
    abi::emit_call_label(emitter, "__rt_unser_release_payload");

    // -- move the new value into the existing box --
    abi::load_at_offset(emitter, "x9", 32);
    ins(emitter, "ldr x10, [x9]", "mov r10, QWORD PTR [r9]", "replacement value tag");
    abi::store_at_offset(emitter, "x10", 48);
    ins(emitter, "ldr x10, [x9, #8]", "mov r10, QWORD PTR [r9 + 8]", "replacement payload low word");
    abi::store_at_offset(emitter, "x10", 56);
    ins(emitter, "ldr x10, [x9, #16]", "mov r10, QWORD PTR [r9 + 16]", "replacement payload high word");
    abi::store_at_offset(emitter, "x10", 64);
    abi::load_at_offset(emitter, "x9", 40);
    abi::load_at_offset(emitter, "x10", 48);
    ins(emitter, "str x10, [x9]", "mov QWORD PTR [r9], r10", "publish the replacement tag in the existing box");
    abi::load_at_offset(emitter, "x10", 56);
    ins(emitter, "str x10, [x9, #8]", "mov QWORD PTR [r9 + 8], r10", "publish the replacement low payload word");
    abi::load_at_offset(emitter, "x10", 64);
    ins(emitter, "str x10, [x9, #16]", "mov QWORD PTR [r9 + 16], r10", "publish the replacement high payload word");

    // -- retire the parsed box; containers keep a wrapper at their own reference slot --
    abi::load_at_offset(emitter, "x10", 48);
    ins(emitter, "cmp x10, #4", "cmp r10, 4", "is the replacement an indexed array or hash?");
    ins(emitter, "b.lo __rt_unser_store_duplicate_scalar", "jl __rt_unser_store_duplicate_scalar_x", "scalars have no reference slot to preserve");
    ins(emitter, "cmp x10, #5", "cmp r10, 5", "does the replacement use container storage?");
    ins(emitter, "b.ls __rt_unser_store_duplicate_container", "jle __rt_unser_store_duplicate_container_x", "arrays and hashes own retained payload storage");
    ins(emitter, "cmp x10, #6", "cmp r10, 6", "is the replacement an object?");
    ins(emitter, "b.eq __rt_unser_store_duplicate_object", "je __rt_unser_store_duplicate_object_x", "objects need a retained nested wrapper");
    ins(emitter, "b __rt_unser_store_duplicate_scalar", "jmp __rt_unser_store_duplicate_scalar_x", "other wire values cannot be published references");

    emitter.label("__rt_unser_store_duplicate_container");
    abi::load_at_offset(emitter, "x9", 32);
    ins(emitter, "mov x10, #7", "mov r10, 7", "nested Mixed wrapper tag");
    ins(emitter, "str x10, [x9]", "mov QWORD PTR [r9], r10", "repurpose the parsed box as a wrapper");
    abi::load_at_offset(emitter, "x10", 40);
    ins(emitter, "str x10, [x9, #8]", "mov QWORD PTR [r9 + 8], r10", "point the wrapper at the reused box");
    ins(emitter, "str xzr, [x9, #16]", "mov QWORD PTR [r9 + 16], 0", "nested wrappers carry no high payload word");
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    ins(emitter, "b __rt_unser_store_duplicate_return", "jmp __rt_unser_store_duplicate_return_x", "preserve the wrapper published by the array parser");

    emitter.label("__rt_unser_store_duplicate_object");
    abi::load_at_offset(emitter, "x9", 32);
    ins(emitter, "mov x10, #7", "mov r10, 7", "nested Mixed wrapper tag");
    ins(emitter, "str x10, [x9]", "mov QWORD PTR [r9], r10", "repurpose the parsed box as a wrapper");
    abi::load_at_offset(emitter, "x10", 40);
    ins(emitter, "str x10, [x9, #8]", "mov QWORD PTR [r9 + 8], r10", "point the wrapper at the reused box");
    ins(emitter, "str xzr, [x9, #16]", "mov QWORD PTR [r9 + 16], 0", "nested wrappers carry no high payload word");
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    ins(emitter, "b __rt_unser_store_duplicate_return", "jmp __rt_unser_store_duplicate_return_x", "keep the duplicate object's reference slot valid");

    emitter.label("__rt_unser_store_duplicate_scalar");
    abi::load_at_offset(emitter, "x9", 32);
    ins(emitter, "mov x10, #8", "mov r10, 8", "canonical null tag");
    ins(emitter, "str x10, [x9]", "mov QWORD PTR [r9], r10", "clear the discarded box payload");
    ins(emitter, "str xzr, [x9, #8]", "mov QWORD PTR [r9 + 8], 0", "null has no low payload word");
    ins(emitter, "str xzr, [x9, #16]", "mov QWORD PTR [r9 + 16], 0", "null has no high payload word");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    emitter.label("__rt_unser_store_duplicate_return");
    abi::load_at_offset(emitter, "x0", 8);
    ins(emitter, "b __rt_unser_store_done", "jmp __rt_unser_store_done_x", "return the original hash after an in-place duplicate update");

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
    abi::load_at_offset(emitter, "r9", 40);
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // previous value tag
    abi::store_at_offset(emitter, "r10", 48);
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // previous payload low word
    abi::store_at_offset(emitter, "r10", 56);
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // previous payload high word
    abi::store_at_offset(emitter, "r10", 64);

    abi::load_at_offset(emitter, "rax", 48);
    abi::load_at_offset(emitter, "rdi", 56);
    abi::load_at_offset(emitter, "rsi", 64);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, "rax", 72);
    abi::load_at_offset(emitter, "rdi", 72);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    abi::load_at_offset(emitter, "rdi", 48);
    abi::load_at_offset(emitter, "rsi", 56);
    abi::emit_call_label(emitter, "__rt_unser_release_payload");

    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // replacement value tag
    abi::store_at_offset(emitter, "r10", 48);
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // replacement payload low word
    abi::store_at_offset(emitter, "r10", 56);
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // replacement payload high word
    abi::store_at_offset(emitter, "r10", 64);
    abi::load_at_offset(emitter, "r9", 40);
    abi::load_at_offset(emitter, "r10", 48);
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the replacement tag in the existing box
    abi::load_at_offset(emitter, "r10", 56);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // publish the replacement low payload word
    abi::load_at_offset(emitter, "r10", 64);
    emitter.instruction("mov QWORD PTR [r9 + 16], r10");                        // publish the replacement high payload word

    abi::load_at_offset(emitter, "r10", 48);
    emitter.instruction("cmp r10, 4");                                          // is the replacement an indexed array or hash?
    emitter.instruction("jl __rt_unser_store_duplicate_scalar_x");              // scalars have no reference slot to preserve
    emitter.instruction("cmp r10, 5");                                          // does the replacement use container storage?
    emitter.instruction("jle __rt_unser_store_duplicate_container_x");          // arrays and hashes own retained payload storage
    emitter.instruction("cmp r10, 6");                                          // is the replacement an object?
    emitter.instruction("je __rt_unser_store_duplicate_object_x");              // objects need a retained nested wrapper
    emitter.instruction("jmp __rt_unser_store_duplicate_scalar_x");             // other wire values cannot be published references

    emitter.label("__rt_unser_store_duplicate_container_x");
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 7");                               // repurpose the parsed box as a nested Mixed wrapper
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // point the wrapper at the reused box
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("jmp __rt_unser_store_duplicate_return_x");             // preserve the wrapper published by the array parser

    emitter.label("__rt_unser_store_duplicate_object_x");
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 7");                               // repurpose the parsed box as a nested Mixed wrapper
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // point the wrapper at the reused box
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    emitter.instruction("jmp __rt_unser_store_duplicate_return_x");             // keep the duplicate object's reference slot valid

    emitter.label("__rt_unser_store_duplicate_scalar_x");
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 8");                               // canonical null tag
    emitter.instruction("mov QWORD PTR [r9 + 8], 0");                           // null has no low payload word
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // null has no high payload word
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    emitter.label("__rt_unser_store_duplicate_return_x");
    abi::load_at_offset(emitter, "rax", 8);                                    // return the original hash after an in-place duplicate update

    emitter.label("__rt_unser_store_done_x");
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

/// Emits the AArch64 payload-only release helper used when a duplicate key replaces a value.
///
/// Input: x0=boxed value tag, x1=payload low word. Scalar tags own nothing.
fn emit_release_payload_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize replaced-payload release ---");
    emitter.label_global("__rt_unser_release_payload");
    emitter.instruction("cmp x0, #1");                                          // is the replaced payload an owned string?
    emitter.instruction("b.eq __rt_unser_release_payload_string");              // strings release through the safe heap helper
    emitter.instruction("cmp x0, #4");                                          // is the replaced payload below the container tag range?
    emitter.instruction("b.lo __rt_unser_release_payload_done");                // scalars, bools, floats, and null own no payload
    emitter.instruction("cmp x0, #7");                                          // does the replaced tag stay inside the refcounted range?
    emitter.instruction("b.hi __rt_unser_release_payload_done");                // other tags do not come from the wire decoder
    emitter.instruction("mov x0, x1");                                          // pass the replaced container payload to the uniform release dispatcher
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_payload_string");
    emitter.instruction("mov x0, x1");                                          // pass the replaced string payload to the safe free helper
    abi::emit_jump(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_unser_release_payload_done");
    abi::emit_return(emitter);
}

/// Emits the x86_64 payload-only release helper used when a duplicate key replaces a value.
///
/// Input: rdi=boxed value tag, rsi=payload low word. Scalar tags own nothing.
fn emit_release_payload_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize replaced-payload release ---");
    emitter.label_global("__rt_unser_release_payload");
    emitter.instruction("cmp rdi, 1");                                          // is the replaced payload an owned string?
    emitter.instruction("je __rt_unser_release_payload_string_x");              // strings release through the safe heap helper
    emitter.instruction("cmp rdi, 4");                                          // is the replaced payload below the container tag range?
    emitter.instruction("jb __rt_unser_release_payload_done_x");                // scalars, bools, floats, and null own no payload
    emitter.instruction("cmp rdi, 7");                                          // does the replaced tag stay inside the refcounted range?
    emitter.instruction("ja __rt_unser_release_payload_done_x");                // other tags do not come from the wire decoder
    emitter.instruction("mov rax, rsi");                                        // pass the replaced container payload to the uniform release dispatcher
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_payload_string_x");
    emitter.instruction("mov rax, rsi");                                        // pass the replaced string payload to the safe free helper
    abi::emit_jump(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_unser_release_payload_done_x");
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
