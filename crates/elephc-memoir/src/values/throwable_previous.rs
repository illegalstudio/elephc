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
fn x86_64(emitter: &mut impl Emitter) {
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
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::layout::x86_64_heap_kind_word(5))); // materialize the canonical Mixed kind and allocator marker
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

/// Emits slot lookup and borrowed raw-object reads for every supported target.
pub fn emit_throwable_previous(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_read_aarch64(emitter),
        Arch::X86_64 => emit_read_x86_64(emitter),
    }
}

/// Emits ARM64 lookup returning the effective slot in x0 and its boxed flag in x1.
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
    emitter.instruction("and x11, x10, #-8");                                   // remove flags to recover the aligned byte offset
    emitter.instruction("add x0, x0, x11");                                     // locate the physical object-property slot
    emitter.instruction("tbz x10, #1, __rt_throwable_previous_slot_return");    // ordinary properties already expose their effective slot
    emitter.instruction("ldr x0, [x0]");                                        // reference properties point to stable external slot storage
    emitter.label("__rt_throwable_previous_slot_return");
    emitter.instruction("ret");                                                 // return a borrowed effective slot and its representation flag
    emitter.label("__rt_throwable_previous_compact");
    emitter.instruction("add x0, x0, #40");                                     // compact links have one fixed raw-object offset
    emitter.instruction("mov x1, #0");                                          // compact previous links are never Mixed boxes
    emitter.instruction("ret");                                                 // leave the borrowed effective slot in x0
    emitter.label("__rt_throwable_previous_missing");
    emitter.instruction("mov x0, #0");                                          // zero represents a missing effective slot
    emitter.instruction("mov x1, #0");                                          // missing slots have no storage flags
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

/// Emits the same lookup on System V, returning the effective slot in rax and boxed flag in rdx.
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
    emitter.instruction("ret");                                                 // return the borrowed effective slot
    emitter.label("__rt_throwable_previous_missing");
    emitter.instruction("xor eax, eax");                                        // zero represents a missing slot
    emitter.instruction("xor edx, edx");                                        // missing slots have no storage flags
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
