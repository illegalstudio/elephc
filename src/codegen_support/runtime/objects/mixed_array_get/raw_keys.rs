//! Purpose:
//! Preserves boxed offset operands until generic receiver dispatch selects their semantics.
//!
//! Called from:
//! - The target-specific mixed-array getter emitters.
//!
//! Key details:
//! - Existing integer/string pairs remain accepted; high word -2 carries an original cell.
//! - PHP methods receive an independent value snapshot under the existing unwind owner.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the shared boxed-key converter with the private input/output registers of its callers.
pub(super) fn emit_normalizer(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_mixed_array_key_tuple");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve the caller frame and return address
            emitter.instruction("mov x29, sp");                                 // establish the aligned conversion frame
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // keep nested calls aligned to the native ABI
        }
    }
    abi::emit_boxed_hash_key(emitter, abi::HashKeyNormalization::Php,
        "__rt_mixed_array_key_tuple", "_empty_str");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the conversion caller
            emitter.instruction("ret");                                         // return the normalized x1/x2 key pair
        }
        Arch::X86_64 => {
            emitter.instruction("pop rbp");                                     // restore the conversion caller frame
            emitter.instruction("ret");                                         // return the normalized rsi/rdx key pair
        }
    }
}

/// Defers all object keys while preparing array, scalar and string receiver paths.
pub(super) fn prepare_receiver_key(emitter: &mut Emitter) {
    let normalize = "__rt_mixed_array_get_prepare_key";
    let done = "__rt_mixed_array_get_prepared_key";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #40]");                           // read the caller's canonical-key mode
            emitter.instruction(&format!("tbnz x9, #4, {done}"));               // a completed tuple must not be normalized again
            emitter.instruction(&format!("cbz x0, {normalize}"));               // null receivers retain their existing key evaluation
            emitter.instruction("ldr x9, [x0]");                                // inspect the concrete receiver before key normalization
            emitter.instruction("cmp x9, #6");                                  // object methods define their own offset argument rules
            emitter.instruction(&format!("b.eq {done}"));                       // retain the original object key representation
        }
        Arch::X86_64 => {
            emitter.instruction("test QWORD PTR [rbp - 32], 16");               // read the caller's canonical-key mode
            emitter.instruction(&format!("jnz {done}"));                        // a completed tuple must not be normalized again
            emitter.instruction("test rdi, rdi");                               // null receivers retain their existing key evaluation
            emitter.instruction(&format!("je {normalize}"));                    // normalize before the ordinary null receiver path
            emitter.instruction("cmp QWORD PTR [rdi], 6");                      // object methods define their own offset argument rules
            emitter.instruction(&format!("je {done}"));                         // retain the original object key representation
        }
    }
    emitter.label(normalize);
    normalize_key(emitter, "__rt_mixed_array_get_prepare");
    emitter.label(done);
}

/// Normalizes a saved raw string or boxed pair and restores the frame's saved receiver.
pub(super) fn normalize_key(emitter: &mut Emitter, stem: &str) {
    let boxed = format!("{stem}_boxed");
    let store = format!("{stem}_store");
    let done = format!("{stem}_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x1, [sp, #8]");                            // recover the original low key word
            emitter.instruction("ldr x2, [sp, #16]");                           // recover the original key representation marker
            emitter.instruction(&format!("cmn x2, #{}", -abi::BOXED_ARRAY_KEY));// distinguish original cells from ordinary integer/string pairs
            emitter.instruction(&format!("b.eq {boxed}"));                      // original cells need scalar key classification
            emitter.instruction(&format!("tbnz x2, #63, {done}"));              // legacy integer pairs already have their final key value
            abi::emit_call_label(emitter, "__rt_hash_normalize_key");
            emitter.instruction(&format!("b {store}"));                         // save the selected PHP or exact string key
            emitter.label(&boxed);
            emitter.instruction("mov x0, x1");                                  // pass the original boxed key to the shared converter
            abi::emit_call_label(emitter, "__rt_mixed_array_key_tuple");
            emitter.label(&store);
            emitter.instruction("str x1, [sp, #8]");                            // keep the normalized low word across receiver dispatch
            emitter.instruction("str x2, [sp, #16]");                           // keep the normalized high word across receiver dispatch
            emitter.label(&done);
            emitter.instruction("ldr x0, [sp, #0]");                            // restore the saved receiver after conversion callbacks
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // recover the original low key word
            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");               // recover the original key representation marker
            emitter.instruction(&format!("cmp rdx, {}", abi::BOXED_ARRAY_KEY)); // distinguish original cells from ordinary integer/string pairs
            emitter.instruction(&format!("je {boxed}"));                        // original cells need scalar key classification
            emitter.instruction("test rdx, rdx");                               // legacy integer pairs use a negative high word
            emitter.instruction(&format!("js {done}"));                         // keep already normalized integer keys
            emitter.instruction("mov rax, rsi");                                // pass the original string pointer to the string classifier
            abi::emit_call_label(emitter, "__rt_hash_normalize_key");
            emitter.instruction("mov rsi, rax");                                // recover the selected low key word
            emitter.instruction(&format!("jmp {store}"));                       // save the selected PHP or exact string key
            emitter.label(&boxed);
            emitter.instruction("mov rax, rsi");                                // pass the original boxed key to the shared converter
            abi::emit_call_label(emitter, "__rt_mixed_array_key_tuple");
            emitter.label(&store);
            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // keep the normalized low word across receiver dispatch
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // keep the normalized high word across receiver dispatch
            emitter.label(&done);
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // restore the saved receiver after conversion callbacks
        }
    }
}

/// Copies a dereferenced original value before lending it to a PHP offset getter.
pub(super) fn clone_original_key(emitter: &mut Emitter) {
    let legacy = "__rt_mixed_array_get_php_legacy_key";
    let ready = "__rt_mixed_array_get_php_key_ready";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x11, [sp, #16]");                          // inspect the saved key representation marker
            emitter.instruction(&format!("cmn x11, #{}", -abi::BOXED_ARRAY_KEY));// only original boxed values need a snapshot
            emitter.instruction(&format!("b.ne {legacy}"));                     // retain legacy integer/string key boxing
            emitter.instruction("ldr x0, [sp, #8]");                            // recover the original key cell
            abi::emit_call_label(emitter, "__rt_mixed_deref");
            abi::emit_call_label(emitter, "__rt_mixed_clone");
            emitter.instruction(&format!("b {ready}"));                         // share the existing key owner and callback cleanup
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp QWORD PTR [rbp - 24], {}", abi::BOXED_ARRAY_KEY));// inspect the saved key representation marker
            emitter.instruction(&format!("jne {legacy}"));                      // retain legacy integer/string key boxing
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // recover the original key cell
            abi::emit_call_label(emitter, "__rt_mixed_deref");
            abi::emit_call_label(emitter, "__rt_mixed_clone");
            emitter.instruction(&format!("jmp {ready}"));                       // share the existing key owner and callback cleanup
        }
    }
}
