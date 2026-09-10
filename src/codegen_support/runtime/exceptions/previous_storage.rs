//! Purpose:
//! Reads and fills the inherited previous slot in native Throwable storage.
//!
//! Called from:
//! - Native cleanup exception chaining and Throwable::getPrevious lowering.
//!
//! Key details:
//! - Heap kind 6 stores a raw previous object; ordinary subclasses store a boxed nullable value.
//! - Both layouts preserve the inherited slot at offset 40; insertion consumes the previous owner.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits borrowed previous reads and ownership-transferring insertion for both native layouts.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
}

/// Adapts compact and ordinary Throwable payloads using AArch64 native arguments.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_throwable_previous");
    emitter.instruction("ldrb w9, [x0, #-8]");                                  // inspect the owner heap kind without touching the inherited property
    emitter.instruction("ldr x0, [x0, #40]");                                   // load the raw previous object or its nullable Mixed cell
    emitter.instruction("cmp w9, #6");                                          // recognize compact Throwable storage
    emitter.instruction("b.eq __rt_throwable_previous_done");                   // compact previous pointers need no unboxing
    emitter.instruction("cbz x0, __rt_throwable_previous_done");                // zero-initialized ordinary properties have no previous owner
    emitter.instruction("ldr x9, [x0]");                                        // inspect the nullable property runtime tag
    emitter.instruction("cmp x9, #6");                                          // only an object-valued property contains a previous Throwable
    emitter.instruction("b.ne __rt_throwable_previous_null");                   // normalize boxed null to the raw absent-pointer convention
    emitter.instruction("ldr x0, [x0, #8]");                                    // borrow the previous object from its property cell
    emitter.label("__rt_throwable_previous_done");
    emitter.instruction("ret");                                                 // return a borrowed previous object without changing ownership
    emitter.label("__rt_throwable_previous_null");
    emitter.instruction("mov x0, #0");                                          // represent an absent previous object with zero
    emitter.instruction("ret");                                                 // return without retaining the nullable property
    emitter.label_global("__rt_throwable_append_previous");
    emitter.instruction("ldrb w9, [x0, #-8]");                                  // distinguish compact raw ownership from a boxed property
    emitter.instruction("cmp w9, #6");                                          // check whether insertion can transfer the raw previous pointer
    emitter.instruction("b.eq __rt_throwable_append_previous_raw");             // compact payloads require no cell allocation
    emitter.instruction("sub sp, sp, #48");                                     // reserve owner, previous value, replaced cell, and native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // retain native linkage across allocation and null-cell release
    emitter.instruction("add x29, sp, #32");                                    // establish a stable insertion frame
    emitter.instruction("stp x0, x1, [sp]");                                    // retain the destination and transferred previous owner
    emitter.instruction("ldr x9, [x0, #40]");                                   // retain the existing null cell until replacement is installed
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the nullable cell across allocation
    emitter.instruction("mov x0, #24");                                         // allocate a complete three-word Mixed object cell
    emitter.instruction("bl __rt_heap_alloc");                                  // reserve independent storage without retaining the transferred child
    emitter.instruction("mov x9, #5");                                          // stamp the box as Mixed heap storage
    emitter.instruction("str x9, [x0, #-8]");                                   // install the canonical Mixed heap kind
    emitter.instruction("mov x9, #6");                                          // identify the boxed child as an object
    emitter.instruction("str x9, [x0]");                                        // store the runtime object tag
    emitter.instruction("ldp x9, x10, [sp]");                                   // recover destination and transferred previous owner
    emitter.instruction("str x10, [x0, #8]");                                   // consume the previous owner into its new property cell
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the unused object payload word
    emitter.instruction("str x0, [x9, #40]");                                   // publish the independent previous property cell
    emitter.instruction("str xzr, [x9, #48]");                                  // clear the inherited property high word
    emitter.instruction("ldr x0, [sp, #16]");                                   // consume the replaced null cell without affecting its aliases
    emitter.instruction("bl __rt_decref_any");                                  // release the old null cell after publishing the new owner
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore native linkage
    emitter.instruction("add sp, sp, #48");                                     // release insertion staging storage
    emitter.instruction("ret");                                                 // return after transferring exactly one previous owner
    emitter.label("__rt_throwable_append_previous_raw");
    emitter.instruction("str x1, [x0, #40]");                                   // transfer the previous owner directly into compact Throwable storage
    emitter.instruction("str xzr, [x0, #48]");                                  // clear the unused high word of the compact previous slot
    emitter.instruction("ret");                                                 // finish insertion without an additional retain
}

/// Applies the same previous-slot representation and owner-transfer rules under SysV.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_throwable_previous");
    emitter.instruction("movzx r10d, BYTE PTR [rax - 8]");                      // inspect the owner heap kind
    emitter.instruction("mov rax, QWORD PTR [rax + 40]");                       // load the raw object or boxed nullable previous value
    emitter.instruction("cmp r10d, 6");                                         // recognize compact Throwable storage
    emitter.instruction("je __rt_throwable_previous_done");                     // return compact raw ownership without unboxing
    emitter.instruction("test rax, rax");                                       // recognize an uninitialized empty ordinary previous slot
    emitter.instruction("jz __rt_throwable_previous_done");                     // return zero for an empty previous property
    emitter.instruction("cmp QWORD PTR [rax], 6");                              // check whether the nullable property contains an object
    emitter.instruction("jne __rt_throwable_previous_null");                    // normalize boxed null to zero
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // borrow the previous object from its Mixed cell
    emitter.label("__rt_throwable_previous_done");
    emitter.instruction("ret");                                                 // return borrowed previous ownership
    emitter.label("__rt_throwable_previous_null");
    emitter.instruction("xor eax, eax");                                        // materialize the absent previous pointer
    emitter.instruction("ret");                                                 // leave the null property owner unchanged
    emitter.label_global("__rt_throwable_append_previous");
    emitter.instruction("cmp BYTE PTR [rdi - 8], 6");                           // distinguish compact raw previous storage
    emitter.instruction("je __rt_throwable_append_previous_raw");               // transfer directly when no nullable cell is needed
    emitter.instruction("push rbp");                                            // align the stack and retain the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable insertion staging
    emitter.instruction("sub rsp, 32");                                         // reserve owner, transferred previous, and replaced null cell
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // preserve the destination Throwable
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // preserve transferred previous ownership across allocation
    emitter.instruction("mov r10, QWORD PTR [rdi + 40]");                       // recover the old nullable property cell
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // retain the replaced cell until new storage is published
    emitter.instruction("mov rax, 24");                                         // reserve a full three-word Mixed cell
    emitter.instruction("call __rt_heap_alloc");                                // allocate independent property storage
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the canonical Mixed kind and allocator marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the Mixed cell header
    emitter.instruction("mov QWORD PTR [rax], 6");                              // identify the transferred child as an object
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the consumed previous owner
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // transfer the child into its new Mixed property cell
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the unused object payload word
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // recover the destination Throwable
    emitter.instruction("mov QWORD PTR [r10 + 40], rax");                       // publish the independent previous property
    emitter.instruction("mov QWORD PTR [r10 + 48], 0");                         // clear the unused inherited property high word
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // consume the replaced null cell
    emitter.instruction("call __rt_decref_any");                                // preserve aliases while dropping the replaced property owner
    emitter.instruction("leave");                                               // restore the native caller frame
    emitter.instruction("ret");                                                 // return after consuming exactly one previous owner
    emitter.label("__rt_throwable_append_previous_raw");
    emitter.instruction("mov QWORD PTR [rdi + 40], rsi");                       // transfer the raw previous owner into compact storage
    emitter.instruction("mov QWORD PTR [rdi + 48], 0");                         // clear the unused compact previous high word
    emitter.instruction("ret");                                                 // return without changing the transferred owner count
}
