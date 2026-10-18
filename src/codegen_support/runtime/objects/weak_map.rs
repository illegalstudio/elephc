//! Purpose:
//! Notifies WeakMap observers after final key destruction and before property reclamation.
//!
//! Called from:
//! - The protected object deep-cleanup path after resurrection has been ruled out.
//!
//! Key details:
//! - Only keys with a live WeakReference can have map observers.
//! - The PHP callback is a static EIR method, published by user metadata without heap ownership.
//! - Deep cleanup contains callback exceptions and still completes object reclamation.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits a guarded notification entry with one borrowed object argument in the C ABI.
pub(crate) fn emit_weak_map(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_weak_map_notify");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Invalidates the weak key and invokes its PHP observer callback on AArch64.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve the caller during the optional PHP callback
    emitter.instruction("str x0, [sp, #16]");                                   // retain only the borrowed object address in scratch
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("sub x10, x0, x9");                                     // derive the key's direct-mapped allocation offset
    emitter.instruction("lsr x10, x10, #4");                                    // select its weak observer granule
    abi::emit_symbol_address(emitter, "x9", "_obj_weak_reference_index");
    emitter.instruction("ldr x9, [x9, x10, lsl #3]");                           // only observed keys require PHP cleanup
    emitter.instruction("cbz x9, __rt_weak_map_notify_done");                   // avoid callbacks for unobserved objects
    abi::emit_call_label(emitter, "__rt_weak_reference_retire");
    abi::emit_call_label(emitter, "__rt_object_handle_of");
    emitter.instruction("mov x1, x0");                                          // pass the key handle after the hidden called-class parameter
    abi::emit_load_symbol_to_reg(emitter, "x0", "_weak_map_class_id", 0);
    abi::emit_load_symbol_to_reg(emitter, "x9", "_weak_map_key_retire_fn", 0);
    emitter.instruction("cbz x9, __rt_weak_map_notify_done");                   // programs without live WeakMap methods need no callback
    emitter.instruction("blr x9");                                              // remove this key from every observing map before its bytes are reclaimed
    emitter.label("__rt_weak_map_notify_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // preserve the object identity for enclosing deep cleanup
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore the caller's cleanup frame
    emitter.instruction("ret");                                                 // return after normal callback completion
}

/// Uses identical retirement ordering with System V argument placement on x86_64.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // preserve the caller's cleanup frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor the borrowed object across PHP callback execution
    emitter.instruction("sub rsp, 16");                                         // reserve aligned receiver scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the borrowed C-ABI object argument
    emitter.instruction("mov rax, rdi");                                        // select the internal runtime pointer convention
    abi::emit_symbol_address(emitter, "r11", "_heap_buf");
    emitter.instruction("mov r10, rax");                                        // copy the key before deriving its granule
    emitter.instruction("sub r10, r11");                                        // derive the key's heap-relative offset
    emitter.instruction("shr r10, 4");                                          // select its weak observer granule
    abi::emit_symbol_address(emitter, "r11", "_obj_weak_reference_index");
    emitter.instruction("cmp QWORD PTR [r11 + r10 * 8], 0");                    // unobserved keys have no maps to notify
    emitter.instruction("je __rt_weak_map_notify_done");                        // skip PHP callbacks for ordinary object reclamation
    abi::emit_call_label(emitter, "__rt_weak_reference_retire");
    abi::emit_call_label(emitter, "__rt_object_handle_of");
    emitter.instruction("mov rsi, rax");                                        // pass the key handle after the hidden called-class parameter
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_weak_map_class_id", 0);
    abi::emit_load_symbol_to_reg(emitter, "r11", "_weak_map_key_retire_fn", 0);
    emitter.instruction("test r11, r11");                                       // a pruned WeakMap surface publishes no callback
    emitter.instruction("jz __rt_weak_map_notify_done");                        // skip callback execution when no map can observe this key
    emitter.instruction("call r11");                                            // remove observing entries before final key storage reclamation
    emitter.label("__rt_weak_map_notify_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the enclosing cleanup's object identity
    emitter.instruction("leave");                                               // release scratch and restore the caller frame
    emitter.instruction("ret");                                                 // return after normal callback completion
}
