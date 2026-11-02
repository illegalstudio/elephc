//! Purpose:
//! Reads and fills the inherited previous slot in native Throwable storage.
//!
//! Called from:
//! - Native cleanup exception chaining and Throwable::getPrevious lowering.
//!
//! Key details:
//! - Every Throwable locates its effective slot through `__rt_throwable_previous_slot` (class
//!   metadata), which reports whether the declared slot owns a Mixed box; this tree has no
//!   fixed-offset compact Throwable layout.
//! - Insertion consumes the previous owner; a class without a previous slot releases it.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits borrowed previous reads and ownership-transferring insertion for both native layouts.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
}

/// Adapts compact and ordinary Throwable payloads using AArch64 native arguments.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_throwable_append_previous");
    emitter.instruction("sub sp, sp, #48");                                     // reserve slot, previous value, replaced cell, and native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // retain native linkage across allocation and null-cell release
    emitter.instruction("add x29, sp, #32");                                    // establish a stable insertion frame
    emitter.instruction("stp x0, x1, [sp]");                                    // retain the destination and transferred previous owner
    emitter.instruction("bl __rt_throwable_previous_slot");                     // locate the effective previous slot from class metadata
    emitter.instruction("cbz x0, __rt_throwable_append_previous_missing");      // a class without the slot cannot keep the link
    emitter.instruction("str x0, [sp]");                                        // keep the effective slot instead of its owner
    emitter.instruction("cbz x1, __rt_throwable_append_previous_slot_raw");     // a raw object slot takes the owner directly
    emitter.instruction("ldr x9, [x0]");                                        // retain the existing null cell until replacement is installed
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the nullable cell across allocation
    emitter.instruction("mov x0, #24");                                         // allocate a complete three-word Mixed object cell
    emitter.instruction("bl __rt_heap_alloc");                                  // reserve independent storage without retaining the transferred child
    emitter.instruction("mov x9, #5");                                          // stamp the box as Mixed heap storage
    emitter.instruction("str x9, [x0, #-8]");                                   // install the canonical Mixed heap kind
    emitter.instruction("mov x9, #6");                                          // identify the boxed child as an object
    emitter.instruction("str x9, [x0]");                                        // store the runtime object tag
    emitter.instruction("ldp x9, x10, [sp]");                                   // recover the effective slot and transferred previous owner
    emitter.instruction("str x10, [x0, #8]");                                   // consume the previous owner into its new property cell
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the unused object payload word
    emitter.instruction("str x0, [x9]");                                        // publish the independent previous property cell
    emitter.instruction("str xzr, [x9, #8]");                                   // clear the inherited property high word
    emitter.instruction("ldr x0, [sp, #16]");                                   // consume the replaced null cell without affecting its aliases
    emitter.instruction("bl __rt_decref_any");                                  // release the old null cell after publishing the new owner
    emitter.instruction("b __rt_throwable_append_previous_done");               // the owner now lives in the new cell
    emitter.label("__rt_throwable_append_previous_slot_raw");
    emitter.instruction("ldr x10, [sp, #8]");                                   // recover the transferred previous owner
    emitter.instruction("str x10, [x0]");                                       // the raw slot at the chain end held no owner
    emitter.instruction("b __rt_throwable_append_previous_done");               // the owner moved into the raw slot
    emitter.label("__rt_throwable_append_previous_missing");
    emitter.instruction("ldr x0, [sp, #8]");                                    // recover the owner nothing can keep
    emitter.instruction("bl __rt_decref_any");                                  // consume it so insertion always takes one owner
    emitter.label("__rt_throwable_append_previous_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore native linkage
    emitter.instruction("add sp, sp, #48");                                     // release insertion staging storage
    emitter.instruction("ret");                                                 // return after transferring exactly one previous owner
}

/// Applies the same previous-slot representation and owner-transfer rules under SysV.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_throwable_append_previous");
    emitter.instruction("push rbp");                                            // align the stack and retain the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable insertion staging
    emitter.instruction("sub rsp, 32");                                         // reserve slot, transferred previous, and replaced null cell
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // preserve transferred previous ownership across helper calls
    emitter.instruction("mov rax, rdi");                                        // pass the destination Throwable to the slot lookup
    emitter.instruction("call __rt_throwable_previous_slot");                   // locate the effective previous slot from class metadata
    emitter.instruction("test rax, rax");                                       // does this class keep a previous slot?
    emitter.instruction("jz __rt_throwable_append_previous_missing");           // a class without the slot cannot keep the link
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // keep the effective slot across allocation
    emitter.instruction("test edx, edx");                                       // inspect the slot representation
    emitter.instruction("jz __rt_throwable_append_previous_slot_raw");          // a raw object slot takes the owner directly
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // recover the old nullable property cell
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // retain the replaced cell until new storage is published
    emitter.instruction("mov rax, 24");                                         // reserve a full three-word Mixed cell
    emitter.instruction("call __rt_heap_alloc");                                // allocate independent property storage
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the canonical Mixed kind and allocator marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the Mixed cell header
    emitter.instruction("mov QWORD PTR [rax], 6");                              // identify the transferred child as an object
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the consumed previous owner
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // transfer the child into its new Mixed property cell
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the unused object payload word
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the effective slot
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the independent previous property
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // clear the unused inherited property high word
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // consume the replaced null cell
    emitter.instruction("call __rt_decref_any");                                // preserve aliases while dropping the replaced property owner
    emitter.instruction("jmp __rt_throwable_append_previous_done");             // the owner now lives in the new cell
    emitter.label("__rt_throwable_append_previous_slot_raw");
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the transferred previous owner
    emitter.instruction("mov QWORD PTR [rax], r10");                            // the raw slot at the chain end held no owner
    emitter.instruction("jmp __rt_throwable_append_previous_done");             // the owner moved into the raw slot
    emitter.label("__rt_throwable_append_previous_missing");
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // recover the owner nothing can keep
    emitter.instruction("call __rt_decref_any");                                // consume it so insertion always takes one owner
    emitter.label("__rt_throwable_append_previous_done");
    emitter.instruction("leave");                                               // restore the native caller frame
    emitter.instruction("ret");                                                 // return after consuming exactly one previous owner
}
