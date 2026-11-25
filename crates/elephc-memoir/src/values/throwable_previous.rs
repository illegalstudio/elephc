//! Purpose:
//! Locates, reads and updates the inherited previous slot in native Throwable storage.
//!
//! Called from:
//! - Throwable accessors and native exception chaining.
//!
//! Key details:
//! - Heap kind 6 stores a raw previous object; ordinary subclasses store a boxed nullable value.
//! - Both layouts preserve the inherited slot at offset 40; insertion consumes the previous owner.

use crate::layout::{emit_branch_if_null_container, object_declared_slot_offset};
use crate::{abi, Arch, Emitter};

/// Runtime symbol holding the number of class-indexed previous-slot descriptors.
pub const SLOT_COUNT_SYMBOL: &str = "_class_previous_slot_count";

/// Runtime symbol holding the sparse class-indexed previous-slot descriptors.
pub const SLOT_DESCRIPTORS_SYMBOL: &str = "_class_previous_slots";

/// Encodes an ordinary object's declared slot and its physical representation flags.
pub const fn slot_descriptor(index: usize, boxed: bool, reference: bool) -> u64 {
    object_declared_slot_offset(index) as u64 | boxed as u64 | ((reference as u64) << 1)
}

/// Emits a transfer of one previous owner into an empty compact or boxed previous slot.
pub fn emit_append_previous(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => aarch64(emitter),
        Arch::X86_64 => x86_64(emitter),
    }
}

/// Adapts compact and ordinary Throwable payloads using AArch64 native arguments.
fn aarch64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_throwable_append_previous");
    emitter.instruction("sub sp, sp, #48");                                     // reserve owner, previous value, replaced cell, and native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // retain native linkage across allocation and null-cell release
    emitter.instruction("add x29, sp, #32");                                    // establish a stable insertion frame
    emitter.instruction("stp x0, x1, [sp]");                                    // retain the destination and transferred previous owner
    emitter.instruction("bl __rt_throwable_previous_slot");                     // resolve compact, ordinary, and reference property storage
    emitter.instruction("cbz x0, __rt_throwable_append_previous_release");      // consume the owner when no writable previous slot exists
    emitter.instruction("cbz x1, __rt_throwable_append_previous_raw");          // raw slots adopt the existing owner directly
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the effective boxed slot across allocation
    emitter.instruction("ldr x9, [x0]");                                        // retain the existing null cell until replacement is installed
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the nullable cell across allocation
    emitter.instruction("ldr x1, [sp, #8]");                                    // supply the owned previous object as the boxed payload
    emitter.instruction("mov x0, #6");                                          // runtime tag six denotes an object payload
    emitter.instruction("mov x2, #0");                                          // object values have no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // create a Mixed box with its own retained object owner
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover the effective previous-property slot
    emitter.instruction("str x0, [x9]");                                        // publish the replacement before retiring displaced owners
    emitter.instruction("ldr x0, [sp, #16]");                                   // consume the replaced null cell without affecting its aliases
    emitter.instruction("bl __rt_decref_any");                                  // release the old null cell after publishing the new owner
    emitter.label("__rt_throwable_append_previous_release");
    emitter.instruction("ldr x0, [sp, #8]");                                    // recover the raw owner retained by the new box or rejected slot
    emitter.instruction("bl __rt_decref_any");                                  // balance that owner after boxing or failed insertion
    emitter.instruction("b __rt_throwable_append_previous_return");             // skip the direct-transfer path
    emitter.label("__rt_throwable_append_previous_raw");
    emitter.instruction("ldr x9, [sp, #8]");                                    // recover the raw previous owner being transferred
    emitter.instruction("cbnz x2, __rt_throwable_append_previous_raw_store");   // reference cells expose only their effective payload word
    emitter.instruction("str xzr, [x0, #8]");                                   // clear the high half of an inline raw-object property
    emitter.label("__rt_throwable_append_previous_raw_store");
    emitter.instruction("str x9, [x0]");                                        // publish the owner in the resolved raw slot
    emitter.label("__rt_throwable_append_previous_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore native linkage
    emitter.instruction("add sp, sp, #48");                                     // release insertion staging storage
    emitter.instruction("ret");                                                 // return after transferring exactly one previous owner
}

/// Applies the same previous-slot representation and owner-transfer rules under SysV.
fn x86_64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_throwable_append_previous");
    emitter.instruction("push rbp");                                            // align the stack and retain the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable insertion staging
    emitter.instruction("sub rsp, 32");                                         // reserve owner, transferred previous, and replaced null cell
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // preserve the destination Throwable
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // preserve transferred previous ownership across allocation
    emitter.instruction("mov rax, rdi");                                        // pass the destination through the runtime result convention
    emitter.instruction("call __rt_throwable_previous_slot");                   // resolve compact, ordinary, and reference property storage
    emitter.instruction("test rax, rax");                                       // check whether a writable previous slot exists
    emitter.instruction("jz __rt_throwable_append_previous_release");           // consume the owner when metadata has no slot
    emitter.instruction("test edx, edx");                                       // inspect the effective slot representation
    emitter.instruction("jz __rt_throwable_append_previous_raw");               // raw slots accept the transferred owner directly
    emitter.instruction("mov QWORD PTR [rsp + 24], rax");                       // preserve the effective boxed slot across allocation
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // recover the old nullable property cell
    emitter.instruction("mov QWORD PTR [rsp + 16], r10");                       // retain the replaced cell until new storage is published
    emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                        // supply the owned previous object as the boxed payload
    emitter.instruction("mov eax, 6");                                          // runtime tag six denotes an object payload
    emitter.instruction("xor esi, esi");                                        // object values have no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // create a Mixed box with its own retained object owner
    emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                       // recover the effective previous-property slot
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the replacement before retiring displaced owners
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // consume the replaced null cell
    emitter.instruction("call __rt_decref_any");                                // preserve aliases while dropping the replaced property owner
    emitter.label("__rt_throwable_append_previous_release");
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // recover the raw owner retained by the box or rejected slot
    emitter.instruction("call __rt_decref_any");                                // balance that owner after boxing or failed insertion
    emitter.instruction("jmp __rt_throwable_append_previous_return");           // skip the direct-transfer path
    emitter.label("__rt_throwable_append_previous_raw");
    emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                        // recover the raw previous owner being transferred
    emitter.instruction("test ecx, ecx");                                       // reference cells expose only their effective payload word
    emitter.instruction("jnz __rt_throwable_append_previous_raw_store");        // preserve the reference cell's adjacent storage
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // clear the high half of an inline raw-object property
    emitter.label("__rt_throwable_append_previous_raw_store");
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the owner in the resolved raw slot
    emitter.label("__rt_throwable_append_previous_return");
    emitter.instruction("leave");                                               // restore the native caller frame
    emitter.instruction("ret");                                                 // return after consuming exactly one previous owner
}

/// Emits slot lookup and borrowed raw-object reads for every supported target.
pub fn emit_throwable_previous(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_read_aarch64(emitter),
        Arch::X86_64 => emit_read_x86_64(emitter),
    }
}

/// Emits ARM64 lookup returning the slot in x0, boxed flag in x1 and reference flag in x2.
fn emit_read_aarch64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_throwable_previous_slot");
    // -- locate either the compact raw link or the ordinary physical property --
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_throwable_previous_missing");
    emitter.instruction("ldr x10, [x0, #-8]");                                  // inspect the concrete heap layout before reading class metadata
    emitter.instruction("and x10, x10, #0xff");                                 // ignore collector and element flags
    emitter.instruction("cmp x10, #6");                                         // compact Throwable allocations use heap kind six
    emitter.instruction("b.eq __rt_throwable_previous_compact");                // compact objects store a raw object pointer at offset forty
    emitter.instruction("ldr x11, [x0]");                                       // preserve the runtime class id across address helpers
    abi::emit_load_symbol_to_reg(emitter, "x10", SLOT_COUNT_SYMBOL, 0);
    emitter.instruction("cmp x11, x10");                                        // reject class ids outside the emitted descriptor table
    emitter.instruction("b.hs __rt_throwable_previous_missing");                // absent classes have no readable previous slot
    abi::emit_symbol_address(emitter, "x10", SLOT_DESCRIPTORS_SYMBOL);
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // load the offset and storage flags for this class
    emitter.instruction("cbz x10, __rt_throwable_previous_missing");            // preserve holes created by reachability pruning
    emitter.instruction("and x1, x10, #1");                                     // expose whether the effective slot owns a Mixed box
    emitter.instruction("ubfx x2, x10, #1, #1");                                // expose whether the slot is reached through a reference cell
    emitter.instruction("and x11, x10, #-8");                                   // remove flags to recover the aligned byte offset
    emitter.instruction("add x0, x0, x11");                                     // locate the physical object-property slot
    emitter.instruction("tbz x10, #1, __rt_throwable_previous_slot_return");    // ordinary properties already expose their effective slot
    emitter.instruction("ldr x0, [x0]");                                        // reference properties point to stable external slot storage
    emitter.label("__rt_throwable_previous_slot_return");
    emitter.instruction("ret");                                                 // return a borrowed effective slot and its representation flag
    emitter.label("__rt_throwable_previous_compact");
    emitter.instruction("add x0, x0, #40");                                     // compact links have one fixed raw-object offset
    emitter.instruction("mov x1, #0");                                          // compact previous links are never Mixed boxes
    emitter.instruction("mov x2, #0");                                          // compact links are inline rather than reference-backed
    emitter.instruction("ret");                                                 // leave the borrowed effective slot in x0
    emitter.label("__rt_throwable_previous_missing");
    emitter.instruction("mov x0, #0");                                          // zero represents a missing effective slot
    emitter.instruction("mov x1, #0");                                          // missing slots have no storage flags
    emitter.instruction("mov x2, #0");                                          // missing slots have no reference indirection
    emitter.instruction("ret");                                                 // never dereference an unknown layout

    emitter.label_global("__rt_throwable_previous");
    // -- read a borrowed raw link, unboxing only declared boxed storage --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller across slot lookup and Mixed unboxing
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("bl __rt_throwable_previous_slot");                     // resolve compact, ordinary, and reference property layouts
    emitter.instruction("cbz x0, __rt_throwable_previous_return");              // no slot means there is no previous exception
    emitter.instruction("ldr x0, [x0]");                                        // borrow the stored raw object or Mixed box
    emitter.instruction("cbz x1, __rt_throwable_previous_raw");                 // raw links must not be interpreted as Mixed tags
    emitter.instruction("bl __rt_mixed_unbox");                                 // follow nested boxes and canonicalize nullable payloads
    emitter.instruction("cmp x0, #6");                                          // only an object contributes a previous-exception link
    emitter.instruction("csel x0, x1, xzr, eq");                                // return the borrowed object or canonical zero
    emitter.instruction("b __rt_throwable_previous_return");                    // skip raw-pointer null normalization after unboxing
    emitter.label("__rt_throwable_previous_raw");
    emit_branch_if_null_container(emitter, "x0", "x9", "__rt_throwable_previous_null");
    emitter.instruction("b __rt_throwable_previous_return");                    // retain the live borrowed raw-object pointer
    emitter.label("__rt_throwable_previous_null");
    emitter.instruction("mov x0, #0");                                          // canonicalize raw nullable sentinels for chain traversal
    emitter.label("__rt_throwable_previous_return");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller without changing ownership
    emitter.instruction("ret");                                                 // return the borrowed raw previous object or zero
}

/// Emits the same lookup on System V, returning the slot in rax, boxed flag in edx and reference flag in ecx.
fn emit_read_x86_64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_throwable_previous_slot");
    // -- locate either the compact raw link or the ordinary physical property --
    emit_branch_if_null_container(emitter, "rax", "r10", "__rt_throwable_previous_missing");
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // inspect concrete heap layout before reading class metadata
    emitter.instruction("and r10d, 0xff");                                      // ignore collector and element flags
    emitter.instruction("cmp r10d, 6");                                         // compact Throwable allocations use heap kind six
    emitter.instruction("je __rt_throwable_previous_compact");                  // compact objects use the fixed raw-object link
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // preserve the runtime class id across address helpers
    abi::emit_load_symbol_to_reg(emitter, "r10", SLOT_COUNT_SYMBOL, 0);
    emitter.instruction("cmp r11, r10");                                        // bound the class-id-indexed metadata read
    emitter.instruction("jae __rt_throwable_previous_missing");                 // unknown classes have no readable previous slot
    abi::emit_symbol_address(emitter, "r10", SLOT_DESCRIPTORS_SYMBOL);
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // recover this class's offset and storage flags
    emitter.instruction("test r10, r10");                                       // inspect the reachability-pruned descriptor
    emitter.instruction("jz __rt_throwable_previous_missing");                  // preserve holes in the class-id table
    emitter.instruction("mov rdx, r10");                                        // copy descriptor bits before discarding the offset
    emitter.instruction("and edx, 1");                                          // expose whether the effective slot owns a Mixed box
    emitter.instruction("mov rcx, r10");                                        // copy the reference-indirection flag for owner publication
    emitter.instruction("and ecx, 2");                                          // preserve whether the resolved slot is reference-backed
    emitter.instruction("mov r11, r10");                                        // preserve reference indirection while extracting the offset
    emitter.instruction("and r11, -8");                                         // remove flags from the aligned byte offset
    emitter.instruction("add rax, r11");                                        // locate the physical object-property slot
    emitter.instruction("test r10b, 2");                                        // does the property store a reference-slot pointer?
    emitter.instruction("jz __rt_throwable_previous_slot_return");              // non-reference properties already expose the effective slot
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // follow the reference without replacing its identity
    emitter.label("__rt_throwable_previous_slot_return");
    emitter.instruction("ret");                                                 // return the borrowed slot and its representation flag
    emitter.label("__rt_throwable_previous_compact");
    emitter.instruction("add rax, 40");                                         // locate the compact raw-object previous link
    emitter.instruction("xor edx, edx");                                        // compact links never own Mixed boxes
    emitter.instruction("xor ecx, ecx");                                        // compact links are inline rather than reference-backed
    emitter.instruction("ret");                                                 // return the borrowed effective slot
    emitter.label("__rt_throwable_previous_missing");
    emitter.instruction("xor eax, eax");                                        // zero represents a missing slot
    emitter.instruction("xor edx, edx");                                        // missing slots have no storage flags
    emitter.instruction("xor ecx, ecx");                                        // missing slots have no reference indirection
    emitter.instruction("ret");                                                 // avoid dereferencing unknown class layouts

    emitter.label_global("__rt_throwable_previous");
    // -- read a borrowed raw link, unboxing only declared boxed storage --
    emitter.instruction("push rbp");                                            // preserve the caller and align the stack for helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("call __rt_throwable_previous_slot");                   // resolve compact, ordinary, and reference layouts
    emitter.instruction("test rax, rax");                                       // check whether a previous slot exists
    emitter.instruction("jz __rt_throwable_previous_return");                   // no slot means no previous exception
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // borrow the stored raw object or Mixed box
    emitter.instruction("test edx, edx");                                       // inspect the resolved storage representation
    emitter.instruction("jz __rt_throwable_previous_raw");                      // raw object links must not be treated as boxed tags
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells and canonicalize nullable payloads
    emitter.instruction("cmp rax, 6");                                          // only an object supplies a previous-exception link
    emitter.instruction("mov eax, 0");                                          // default to null without changing the comparison flags
    emitter.instruction("cmove rax, rdi");                                      // select the borrowed object payload when its tag matches
    emitter.instruction("jmp __rt_throwable_previous_return");                  // skip raw-pointer normalization after unboxing
    emitter.label("__rt_throwable_previous_raw");
    emit_branch_if_null_container(emitter, "rax", "r10", "__rt_throwable_previous_null");
    emitter.instruction("jmp __rt_throwable_previous_return");                  // preserve a live borrowed raw-object pointer
    emitter.label("__rt_throwable_previous_null");
    emitter.instruction("xor eax, eax");                                        // normalize raw nullable sentinels for chain traversal
    emitter.label("__rt_throwable_previous_return");
    emitter.instruction("pop rbp");                                             // restore the caller without changing ownership
    emitter.instruction("ret");                                                 // return the borrowed raw previous object or zero
}
