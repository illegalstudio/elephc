//! Purpose:
//! Emits the ownership finisher used after a parsed value is stored into a typed property slot.
//!
//! Called from:
//! - `__rt_obj_store_prop` in the target-specific object-storage helpers.
//!
//! Key details:
//! - Scalar and string slots copy the box payload; the box itself must be freed, and a string
//!   pointer transfers its persisted owner to the property slot.
//! - Array, hash, and object slots receive a payload pointer. The property acquires its own
//!   payload retain while the parsed box stays alive until the decoder context completes, so a
//!   later `R:`/`r:` slot keeps addressing live storage.
//! - The helper takes a borrowed box and consumes it in every path.

use crate::codegen_support::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific post-store ownership finisher.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
    match emitter.target.arch {
        Arch::AArch64 => emit_release_previous_aarch64(emitter),
        Arch::X86_64 => emit_release_previous_x86_64(emitter),
    }
    match emitter.target.arch {
        Arch::AArch64 => emit_reuse_box_aarch64(emitter),
        Arch::X86_64 => emit_reuse_box_x86_64(emitter),
    }
}

/// Emits the AArch64 post-store ownership finisher.
///
/// Input: x0 = parsed value box. The property slot already holds its copy or payload pointer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize stored-property ownership ---");
    emitter.label_global("__rt_unser_finish_stored_prop");
    emitter.instruction("cbz x0, __rt_unser_finish_stored_prop_done");          // a missing box needs no cleanup
    emitter.instruction("ldr x9, [x0]");                                        // boxed value tag
    emitter.instruction("cmp x9, #1");                                          // is the stored payload an owned string?
    emitter.instruction("b.eq __rt_unser_finish_stored_prop_string");           // the slot now owns the persisted string
    emitter.instruction("cmp x9, #4");                                          // is the value below the refcounted container range?
    emitter.instruction("b.lo __rt_unser_finish_stored_prop_scalar");           // scalars copy inline and own no heap payload
    emitter.instruction("cmp x9, #6");                                          // does the value stay inside the container range?
    emitter.instruction("b.hi __rt_unser_finish_stored_prop_scalar");           // other wire values transfer no container owner
    emitter.instruction("ldr x9, [x0, #8]");                                    // container payload stored in the property slot
    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the parsed box across the payload retain
    emitter.instruction("mov x0, x9");                                          // pass the property's payload to the retain helper
    emitter.instruction("bl __rt_incref");                                      // give the property slot its own container owner
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the parsed box after the retain
    abi::emit_jump(emitter, "__rt_unserialize_defer_data");                     // keep the registry box alive until the context completes
    emitter.label("__rt_unser_finish_stored_prop_string");
    abi::emit_jump(emitter, "__rt_heap_free");                                  // free the shell; the property slot owns the string payload
    emitter.label("__rt_unser_finish_stored_prop_scalar");
    abi::emit_jump(emitter, "__rt_decref_mixed");                               // release the scalar box through the counted path
    emitter.label("__rt_unser_finish_stored_prop_done");
    abi::emit_return(emitter);
}

/// Emits the x86_64 post-store ownership finisher.
///
/// Input: rax = parsed value box. The property slot already holds its copy or payload pointer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize stored-property ownership ---");
    emitter.label_global("__rt_unser_finish_stored_prop");
    emitter.instruction("test rax, rax");                                       // a missing box needs no cleanup
    emitter.instruction("jz __rt_unser_finish_stored_prop_done_x");             // nothing to release
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // boxed value tag
    emitter.instruction("cmp r10, 1");                                          // is the stored payload an owned string?
    emitter.instruction("je __rt_unser_finish_stored_prop_string_x");           // the slot now owns the persisted string
    emitter.instruction("cmp r10, 4");                                          // is the value below the refcounted container range?
    emitter.instruction("jb __rt_unser_finish_stored_prop_scalar_x");           // scalars copy inline and own no heap payload
    emitter.instruction("cmp r10, 6");                                          // does the value stay inside the container range?
    emitter.instruction("ja __rt_unser_finish_stored_prop_scalar_x");           // other wire values transfer no container owner
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // container payload stored in the property slot
    emitter.instruction("push rax");                                            // preserve the parsed box across the payload retain
    emitter.instruction("mov rax, rdi");                                        // __rt_incref consumes its argument in rax
    emitter.instruction("call __rt_incref");                                    // give the property slot its own container owner
    emitter.instruction("pop rax");                                             // restore the parsed box for the context deferral
    abi::emit_jump(emitter, "__rt_unserialize_defer_data");                     // keep the registry box alive until the context completes
    emitter.label("__rt_unser_finish_stored_prop_string_x");
    abi::emit_jump(emitter, "__rt_heap_free");                                  // free the shell; the property slot owns the string payload
    emitter.label("__rt_unser_finish_stored_prop_scalar_x");
    abi::emit_jump(emitter, "__rt_decref_mixed");                               // release the scalar box through the counted path
    emitter.label("__rt_unser_finish_stored_prop_done_x");
    abi::emit_return(emitter);
}

/// Emits the AArch64 release of the property value a hydration store overwrites.
///
/// Input: x0=slot address, x1=compile-time property tag. Uninitialized typed slots are skipped.
fn emit_release_previous_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize previous-property release ---");
    emitter.label_global("__rt_unser_release_prev_prop");
    emitter.instruction("cbz x0, __rt_unser_release_prev_prop_done");           // a missing slot owns no previous value
    emitter.instruction("ldr x9, [x0, #8]");                                    // slot metadata word (string length or typed marker)
    abi::emit_load_int_immediate(emitter, "x10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x9, x10");                                         // is this typed property still uninitialized?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_done");              // no previous owner exists for an uninitialized slot
    emitter.instruction("ldr x9, [x0]");                                        // previous payload low word
    emitter.instruction("cmp x1, #1");                                          // is the previous value an owned string?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_any");               // strings release through the uniform dispatcher
    emitter.instruction("cmp x1, #4");                                          // is the previous value below the container range?
    emitter.instruction("b.lo __rt_unser_release_prev_prop_done");              // scalar slots own no heap payload
    emitter.instruction("cmp x1, #6");                                          // does the previous value stay inside the container range?
    emitter.instruction("b.ls __rt_unser_release_prev_prop_any");               // arrays, hashes, and objects release uniformly
    emitter.instruction("cmp x1, #7");                                          // is the previous value a boxed Mixed cell?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_mixed");             // Mixed cells stay alive for their reference slot
    emitter.instruction("cmp x1, #10");                                         // is the previous value a callable descriptor?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_callable");          // callable descriptors own capture state
    emitter.instruction("cmp x1, #11");                                         // is the previous value an owned reference cell?
    emitter.instruction("b.eq __rt_unser_release_prev_prop_any");               // reference cells release through uniform dispatch
    emitter.instruction("b __rt_unser_release_prev_prop_done");                 // other tags hold no release obligation here
    emitter.label("__rt_unser_release_prev_prop_any");
    emitter.instruction("mov x0, x9");                                          // pass the previous payload to the uniform release helper
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_prev_prop_mixed");
    abi::emit_return(emitter);                                                  // the Mixed store reuses real boxes and skips sentinels
    emitter.label("__rt_unser_release_prev_prop_callable");
    emitter.instruction("mov x0, x9");                                          // pass the previous callable descriptor to its release helper
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_unser_release_prev_prop_done");
    abi::emit_return(emitter);
}

/// Emits the x86_64 release of the property value a hydration store overwrites.
///
/// Input: rdi=slot address, rsi=compile-time property tag. Uninitialized typed slots are skipped.
fn emit_release_previous_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize previous-property release ---");
    emitter.label_global("__rt_unser_release_prev_prop");
    emitter.instruction("test rdi, rdi");                                       // a missing slot owns no previous value
    emitter.instruction("jz __rt_unser_release_prev_prop_done_x");              // nothing to release
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // slot metadata word (string length or typed marker)
    abi::emit_load_int_immediate(emitter, "r11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp r10, r11");                                        // is this typed property still uninitialized?
    emitter.instruction("je __rt_unser_release_prev_prop_done_x");              // no previous owner exists for an uninitialized slot
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // previous payload low word
    emitter.instruction("cmp rsi, 1");                                          // is the previous value an owned string?
    emitter.instruction("je __rt_unser_release_prev_prop_any_x");               // strings release through the uniform dispatcher
    emitter.instruction("cmp rsi, 4");                                          // is the previous value below the container range?
    emitter.instruction("jb __rt_unser_release_prev_prop_done_x");              // scalar slots own no heap payload
    emitter.instruction("cmp rsi, 6");                                          // does the previous value stay inside the container range?
    emitter.instruction("jbe __rt_unser_release_prev_prop_any_x");              // arrays, hashes, and objects release uniformly
    emitter.instruction("cmp rsi, 7");                                          // is the previous value a boxed Mixed cell?
    emitter.instruction("je __rt_unser_release_prev_prop_mixed_x");             // Mixed cells stay alive for their reference slot
    emitter.instruction("cmp rsi, 10");                                         // is the previous value a callable descriptor?
    emitter.instruction("je __rt_unser_release_prev_prop_callable_x");          // callable descriptors own capture state
    emitter.instruction("cmp rsi, 11");                                         // is the previous value an owned reference cell?
    emitter.instruction("je __rt_unser_release_prev_prop_any_x");               // reference cells release through uniform dispatch
    emitter.instruction("jmp __rt_unser_release_prev_prop_done_x");             // other tags hold no release obligation here
    emitter.label("__rt_unser_release_prev_prop_any_x");
    emitter.instruction("mov rax, r10");                                        // pass the previous payload to the uniform release helper
    abi::emit_jump(emitter, "__rt_decref_any");
    emitter.label("__rt_unser_release_prev_prop_mixed_x");
    abi::emit_return(emitter);                                                  // the Mixed store reuses real boxes and skips sentinels
    emitter.label("__rt_unser_release_prev_prop_callable_x");
    emitter.instruction("mov rax, r10");                                        // pass the previous callable descriptor to its release helper
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");
    emitter.label("__rt_unser_release_prev_prop_done_x");
    abi::emit_return(emitter);
}

/// Emits the AArch64 in-place Mixed box reuse used by duplicate hash keys and properties.
///
/// Input: x0=destination box, x1=parsed replacement box. Output: x0=destination box.
fn emit_reuse_box_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize in-place box reuse ---");
    emitter.label_global("__rt_unser_reuse_box");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "x0", 40);
    abi::store_at_offset(emitter, "x1", 32);
    abi::load_at_offset(emitter, "x9", 40);
    emitter.instruction("ldr x10, [x9]");                                       // previous value tag
    abi::store_at_offset(emitter, "x10", 48);
    emitter.instruction("ldr x10, [x9, #8]");                                   // previous payload low word
    abi::store_at_offset(emitter, "x10", 56);
    emitter.instruction("ldr x10, [x9, #16]");                                  // previous payload high word
    abi::store_at_offset(emitter, "x10", 64);
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

    abi::load_at_offset(emitter, "x9", 32);
    emitter.instruction("ldr x10, [x9]");                                       // replacement value tag
    abi::store_at_offset(emitter, "x10", 48);
    emitter.instruction("ldr x10, [x9, #8]");                                   // replacement payload low word
    abi::store_at_offset(emitter, "x10", 56);
    emitter.instruction("ldr x10, [x9, #16]");                                  // replacement payload high word
    abi::store_at_offset(emitter, "x10", 64);
    abi::load_at_offset(emitter, "x9", 40);
    abi::load_at_offset(emitter, "x10", 48);
    emitter.instruction("str x10, [x9]");                                       // publish the replacement tag in the destination box
    abi::load_at_offset(emitter, "x10", 56);
    emitter.instruction("str x10, [x9, #8]");                                   // publish the replacement low payload word
    abi::load_at_offset(emitter, "x10", 64);
    emitter.instruction("str x10, [x9, #16]");                                  // publish the replacement high payload word

    abi::load_at_offset(emitter, "x10", 48);
    emitter.instruction("cmp x10, #4");                                         // is the replacement an array or hash?
    emitter.instruction("b.lo __rt_unser_reuse_scalar");                        // scalars discard their parsed box
    emitter.instruction("cmp x10, #5");                                         // does the replacement use container storage?
    emitter.instruction("b.ls __rt_unser_reuse_container");                     // arrays and hashes keep a wrapper slot
    emitter.instruction("cmp x10, #6");                                         // is the replacement an object?
    emitter.instruction("b.eq __rt_unser_reuse_object");                        // objects keep a retained wrapper slot
    emitter.instruction("b __rt_unser_reuse_scalar");                           // other wire values discard their box

    emitter.label("__rt_unser_reuse_container");
    abi::load_at_offset(emitter, "x9", 32);
    emitter.instruction("mov x10, #7");                                         // nested Mixed wrapper tag
    emitter.instruction("str x10, [x9]");                                       // repurpose the parsed box as a wrapper
    abi::load_at_offset(emitter, "x10", 40);
    emitter.instruction("str x10, [x9, #8]");                                   // point the wrapper at the reused box
    emitter.instruction("str xzr, [x9, #16]");                                  // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("b __rt_unser_reuse_return");                           // preserve the container's wrapper slot

    emitter.label("__rt_unser_reuse_object");
    abi::load_at_offset(emitter, "x9", 32);
    emitter.instruction("mov x10, #7");                                         // nested Mixed wrapper tag
    emitter.instruction("str x10, [x9]");                                       // repurpose the parsed box as a wrapper
    abi::load_at_offset(emitter, "x10", 40);
    emitter.instruction("str x10, [x9, #8]");                                   // point the wrapper at the reused box
    emitter.instruction("str xzr, [x9, #16]");                                  // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    emitter.instruction("b __rt_unser_reuse_return");                           // keep the duplicate object's reference slot valid

    emitter.label("__rt_unser_reuse_scalar");
    abi::load_at_offset(emitter, "x9", 32);
    emitter.instruction("mov x10, #8");                                         // canonical null tag
    emitter.instruction("str x10, [x9]");                                       // clear the discarded box payload
    emitter.instruction("str xzr, [x9, #8]");                                   // null has no low payload word
    emitter.instruction("str xzr, [x9, #16]");                                  // null has no high payload word
    abi::load_at_offset(emitter, "x0", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    emitter.label("__rt_unser_reuse_return");
    abi::load_at_offset(emitter, "x0", 40);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

/// Emits the x86_64 in-place Mixed box reuse used by duplicate hash keys and properties.
///
/// Input: rdi=destination box, rsi=parsed replacement box. Output: rax=destination box.
fn emit_reuse_box_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize in-place box reuse ---");
    emitter.label_global("__rt_unser_reuse_box");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "rdi", 40);
    abi::store_at_offset(emitter, "rsi", 32);
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
    abi::load_at_offset(emitter, "rax", 72);
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
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the replacement tag in the destination box
    abi::load_at_offset(emitter, "r10", 56);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // publish the replacement low payload word
    abi::load_at_offset(emitter, "r10", 64);
    emitter.instruction("mov QWORD PTR [r9 + 16], r10");                        // publish the replacement high payload word

    abi::load_at_offset(emitter, "r10", 48);
    emitter.instruction("cmp r10, 4");                                          // is the replacement an array or hash?
    emitter.instruction("jl __rt_unser_reuse_scalar_x");                        // scalars discard their parsed box
    emitter.instruction("cmp r10, 5");                                          // does the replacement use container storage?
    emitter.instruction("jle __rt_unser_reuse_container_x");                    // arrays and hashes keep a wrapper slot
    emitter.instruction("cmp r10, 6");                                          // is the replacement an object?
    emitter.instruction("je __rt_unser_reuse_object_x");                        // objects keep a retained wrapper slot
    emitter.instruction("jmp __rt_unser_reuse_scalar_x");                       // other wire values discard their box

    emitter.label("__rt_unser_reuse_container_x");
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 7");                               // repurpose the parsed box as a wrapper
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // point the wrapper at the reused box
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.instruction("jmp __rt_unser_reuse_return_x");                       // preserve the container's wrapper slot

    emitter.label("__rt_unser_reuse_object_x");
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 7");                               // repurpose the parsed box as a wrapper
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("mov QWORD PTR [r9 + 8], r10");                         // point the wrapper at the reused box
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // nested wrappers carry no high payload word
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    emitter.instruction("jmp __rt_unser_reuse_return_x");                       // keep the duplicate object's reference slot valid

    emitter.label("__rt_unser_reuse_scalar_x");
    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("mov QWORD PTR [r9], 8");                               // canonical null tag
    emitter.instruction("mov QWORD PTR [r9 + 8], 0");                           // null has no low payload word
    emitter.instruction("mov QWORD PTR [r9 + 16], 0");                          // null has no high payload word
    abi::load_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    emitter.label("__rt_unser_reuse_return_x");
    abi::load_at_offset(emitter, "rax", 40);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target releases the value a hydration store overwrites, skipping only the
    /// uninitialized typed-property sentinel, and routes each owner kind to its helper.
    #[test]
    fn previous_property_release_contract_holds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let block = asm
                .split_once("__rt_unser_release_prev_prop").unwrap().1
                .split_once("__rt_unser_reuse_box").unwrap().0;
            assert!(block.contains("__rt_decref_any"), "{name}");
            assert!(block.contains("__rt_callable_descriptor_release"), "{name}");
            assert!(!block.contains("__rt_unserialize_defer_data"), "{name}: Mixed slots are handled by the store");
            assert!(asm.contains("__rt_unser_reuse_box"), "{name}");
        }
    }

    /// Every target keeps container payloads alive through the context and frees copied
    /// scalar/string box shells after the property slot has taken what it needs.
    #[test]
    fn stored_property_finisher_contract_holds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_unser_finish_stored_prop"), "{name}");
            assert!(asm.contains("__rt_incref"), "{name}");
            assert!(asm.contains("__rt_unserialize_defer_data"), "{name}");
            assert!(asm.contains("__rt_heap_free"), "{name}");
        }
    }
}
