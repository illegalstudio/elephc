//! Purpose:
//! Fills newly opened splice gaps with packed replacement slots.
//!
//! Called from:
//! - The compiler's native splice runtime registration.
//!
//! Key details:
//! - The logical length is published after the stored replacement slots are initialized.

use crate::{Arch, Emitter};
use super::{emit_open_string_gap, emit_open_word_gap};

/// Emits `__rt_array_splice_insert` for the active target.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = destination indexed array, `x1` = insertion index, `x2` = replacement
///   indexed array (0 inserts nothing)
/// - **Output**: `x0` = the possibly-relocated destination indexed array
///
/// ## x86_64 ABI
/// - **Input**: `rdi` = destination, `rsi` = insertion index, `rdx` = replacement
/// - **Output**: `rax` = the possibly-relocated destination indexed array
///
/// Payload slots are copied verbatim, which is correct for the 8-byte scalar element types
/// (`int`, `bool`, `float`, `callable`) this variant serves.
pub fn emit_array_splice_insert(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_array_splice_insert_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert ---");
    emitter.label_global("__rt_array_splice_insert");

    // Stack layout: [sp,#0] destination array, [sp,#8] replacement array,
    //               [sp,#16] insertion index, [sp,#24] copy loop index,
    //               [sp,#32] payload scratch, [sp,#40] source value_type tag,
    //               [sp,#48] saved x29/x30.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the insertion bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination indexed-array pointer across growth
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the replacement indexed-array pointer across growth
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the requested insertion index across growth
    emitter.instruction("str xzr, [sp, #24]");                                  // start the replacement copy loop at the first replacement slot
    emitter.instruction("cbz x2, __rt_asi_done");                               // a null replacement inserts nothing
    emitter.instruction("ldr x9, [x2]");                                        // x9 = replacement element count
    emitter.instruction("cbz x9, __rt_asi_done");                               // an empty replacement inserts nothing

    // -- propagate the replacement's value_type so an empty destination is tagged --
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("ldr x11, [x2, #-8]");                                  // load the replacement packed array kind word
    emitter.instruction("and x11, x11, #0x7f00");                               // keep only the replacement value_type lane
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the destination kind byte and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // drop stale destination value_type bits before propagation
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination kind bits with the replacement tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the propagated packed array value_type tag

    emit_open_word_gap(emitter, "__rt_asi");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asi_copy");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("cmp x4, x9");                                          // has every replacement element been copied?
    emitter.instruction("b.ge __rt_asi_set_len");                               // yes, publish the extended destination length
    emitter.instruction("add x5, x2, #24");                                     // compute the replacement payload base address
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load the borrowed replacement payload
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the clamped insertion index
    emitter.instruction("add x3, x0, #24");                                     // compute the destination payload base address
    emitter.instruction("add x7, x1, x4");                                      // compute the destination slot for this replacement element
    emitter.instruction("str x6, [x3, x7, lsl #3]");                            // store the replacement payload into the opened gap
    emitter.instruction("add x4, x4, #1");                                      // advance to the next replacement element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the updated replacement copy index
    emitter.instruction("b __rt_asi_copy");                                     // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asi_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("ldr x10, [x0]");                                       // reload the previous destination logical length
    emitter.instruction("add x10, x10, x9");                                    // extend it by the inserted element count
    emitter.instruction("str x10, [x0]");                                       // persist the extended destination logical length

    emitter.label("__rt_asi_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the possibly-relocated destination pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the insertion bookkeeping slots
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits `__rt_array_splice_insert_refcounted` for the active target.
///
/// Same ABI as [`emit_array_splice_insert`], plus one `__rt_incref` per inserted payload so the
/// destination array and the replacement array each own their own reference.
pub fn emit_array_splice_insert_refcounted(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_array_splice_insert_refcounted_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_refcounted ---");
    emitter.label_global("__rt_array_splice_insert_refcounted");

    // Stack layout: [sp,#0] destination array, [sp,#8] replacement array,
    //               [sp,#16] insertion index, [sp,#24] copy loop index,
    //               [sp,#32] payload scratch, [sp,#40] source value_type tag,
    //               [sp,#48] saved x29/x30.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the insertion bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination indexed-array pointer across growth
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the replacement indexed-array pointer across growth
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the requested insertion index across growth
    emitter.instruction("str xzr, [sp, #24]");                                  // start the replacement copy loop at the first replacement slot
    emitter.instruction("cbz x2, __rt_asir_done");                              // a null replacement inserts nothing
    emitter.instruction("ldr x9, [x2]");                                        // x9 = replacement element count
    emitter.instruction("cbz x9, __rt_asir_done");                              // an empty replacement inserts nothing

    // -- propagate the replacement's value_type so an empty destination is tagged --
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("ldr x11, [x2, #-8]");                                  // load the replacement packed array kind word
    emitter.instruction("and x11, x11, #0x7f00");                               // keep only the replacement value_type lane
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the destination kind byte and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // drop stale destination value_type bits before propagation
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination kind bits with the replacement tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the propagated packed array value_type tag

    emit_open_word_gap(emitter, "__rt_asir");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asir_copy");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("cmp x4, x9");                                          // has every replacement element been copied?
    emitter.instruction("b.ge __rt_asir_set_len");                              // yes, publish the extended destination length
    emitter.instruction("add x5, x2, #24");                                     // compute the replacement payload base address
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load the borrowed replacement payload
    emitter.instruction("str x6, [sp, #32]");                                   // preserve the borrowed payload across the retain call
    emitter.instruction("mov x0, x6");                                          // move the borrowed payload into the retain argument register
    emitter.instruction("bl __rt_incref");                                      // retain it before the destination array becomes an owner
    emitter.instruction("ldr x6, [sp, #32]");                                   // reload the retained payload after the retain call
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index after the retain call
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the clamped insertion index
    emitter.instruction("add x3, x0, #24");                                     // compute the destination payload base address
    emitter.instruction("add x7, x1, x4");                                      // compute the destination slot for this replacement element
    emitter.instruction("str x6, [x3, x7, lsl #3]");                            // store the replacement payload into the opened gap
    emitter.instruction("add x4, x4, #1");                                      // advance to the next replacement element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the updated replacement copy index
    emitter.instruction("b __rt_asir_copy");                                    // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asir_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("ldr x10, [x0]");                                       // reload the previous destination logical length
    emitter.instruction("add x10, x10, x9");                                    // extend it by the inserted element count
    emitter.instruction("str x10, [x0]");                                       // persist the extended destination logical length

    emitter.label("__rt_asir_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the possibly-relocated destination pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the insertion bookkeeping slots
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits `__rt_array_splice_insert_boxed` for the active target.
///
/// Takes one extra argument, the replacement's runtime value_type tag in `x3` / `rcx`, and
/// wraps every replacement payload in a fresh `__rt_mixed_from_value` cell, which the destination
/// then owns outright. That is what a heterogeneous `array<mixed>` receiver needs when the
/// replacement is a typed array such as `[7, 8, 9]` or `["a", "b"]`: the replacement keeps its raw
/// slots and is released by the caller's ordinary temporary cleanup. A string replacement
/// (`tag == 1`) reads 16-byte pointer/length slots and hands the borrowed pair to
/// `__rt_mixed_from_value`, which persists the bytes itself, so the Mixed cell never aliases
/// storage the replacement array still owns.
pub fn emit_array_splice_insert_boxed(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_array_splice_insert_boxed_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_boxed ---");
    emitter.label_global("__rt_array_splice_insert_boxed");

    // Stack layout: [sp,#0] destination array, [sp,#8] replacement array,
    //               [sp,#16] insertion index, [sp,#24] copy loop index,
    //               [sp,#32] payload scratch, [sp,#40] source value_type tag,
    //               [sp,#48] saved x29/x30.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the insertion bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination indexed-array pointer across growth
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the replacement indexed-array pointer across growth
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the requested insertion index across growth
    emitter.instruction("str xzr, [sp, #24]");                                  // start the replacement copy loop at the first replacement slot
    emitter.instruction("str x3, [sp, #40]");                                   // preserve the replacement slot value_type tag for the boxing calls
    emitter.instruction("cbz x2, __rt_asib_done");                              // a null replacement inserts nothing
    emitter.instruction("ldr x9, [x2]");                                        // x9 = replacement element count
    emitter.instruction("cbz x9, __rt_asib_done");                              // an empty replacement inserts nothing

    // -- the destination stores boxed Mixed cells whatever the replacement slots hold --
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the destination kind byte and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // drop stale destination value_type bits before restamping
    emitter.instruction("mov x11, #0x700");                                     // runtime value_type tag 7 marks boxed Mixed payload slots
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination kind bits with the Mixed value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the boxed Mixed value_type tag

    emit_open_word_gap(emitter, "__rt_asib");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asib_copy");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("cmp x4, x9");                                          // has every replacement element been copied?
    emitter.instruction("b.ge __rt_asib_set_len");                              // yes, publish the extended destination length
    emitter.instruction("add x5, x2, #24");                                     // compute the replacement payload base address
    emitter.instruction("ldr x8, [sp, #40]");                                   // reload the replacement slot value_type tag
    emitter.instruction("cmp x8, #1");                                          // do the replacement slots hold string pointer/length pairs?
    emitter.instruction("b.eq __rt_asib_copy_string");                          // string slots need a wider load
    emitter.instruction("ldr x1, [x5, x4, lsl #3]");                            // load the raw replacement payload from its 8-byte slot
    emitter.instruction("mov x2, xzr");                                         // scalar Mixed payloads leave the high payload word clear
    emitter.instruction("b __rt_asib_copy_box");                                // the payload words are ready for the Mixed cell allocator
    emitter.label("__rt_asib_copy_string");
    emitter.instruction("add x5, x5, x4, lsl #4");                              // advance to this element's 16-byte string slot
    emitter.instruction("ldr x1, [x5]");                                        // load the borrowed string pointer from the replacement slot
    emitter.instruction("ldr x2, [x5, #8]");                                    // load the borrowed string length from the replacement slot
    emitter.label("__rt_asib_copy_box");
    emitter.instruction("ldr x0, [sp, #40]");                                   // pass the replacement slot value_type tag to the boxer
    emitter.instruction("bl __rt_mixed_from_value");                            // allocate one owned Mixed cell for this element
    emitter.instruction("mov x6, x0");                                          // the fresh Mixed cell is what the destination slot receives
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index after the boxing call
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the clamped insertion index
    emitter.instruction("add x3, x0, #24");                                     // compute the destination payload base address
    emitter.instruction("add x7, x1, x4");                                      // compute the destination slot for this replacement element
    emitter.instruction("str x6, [x3, x7, lsl #3]");                            // store the replacement payload into the opened gap
    emitter.instruction("add x4, x4, #1");                                      // advance to the next replacement element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the updated replacement copy index
    emitter.instruction("b __rt_asib_copy");                                    // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asib_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("ldr x10, [x0]");                                       // reload the previous destination logical length
    emitter.instruction("add x10, x10, x9");                                    // extend it by the inserted element count
    emitter.instruction("str x10, [x0]");                                       // persist the extended destination logical length

    emitter.label("__rt_asib_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the possibly-relocated destination pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the insertion bookkeeping slots
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits `__rt_array_splice_insert_unboxed` for the active target.
///
/// The mirror of the boxed variant: the replacement holds boxed Mixed cells and the destination
/// stores plain integer slots, so every payload is read back through `__rt_mixed_cast_int`. That
/// is the shape an overflow-checked expression produces, `[$x + 1, $x + 2]` is an `array<mixed>`
/// because `ichecked_add` boxes its result, spliced into an `array<int>` receiver. Nothing is
/// retained or released: the destination stores a copy of the integer value, and the replacement
/// array keeps owning its cells.
///
/// `__rt_mixed_cast_int` reads its argument from `x0` / `rax`, it forwards straight into
/// `__rt_mixed_unbox`, whose x86_64 input register is `rax`, not the first SysV argument register.
pub fn emit_array_splice_insert_unboxed(emitter: &mut impl Emitter, convert: &str) {
    if emitter.arch() == Arch::X86_64 {
        emit_array_splice_insert_unboxed_x86_64(emitter, convert);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_unboxed ---");
    emitter.label_global("__rt_array_splice_insert_unboxed");

    // Stack layout: [sp,#0] destination array, [sp,#8] replacement array,
    //               [sp,#16] insertion index, [sp,#24] copy loop index,
    //               [sp,#32] payload scratch, [sp,#40] source value_type tag,
    //               [sp,#48] saved x29/x30.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the insertion bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination indexed-array pointer across growth
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the replacement indexed-array pointer across growth
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the requested insertion index across growth
    emitter.instruction("str xzr, [sp, #24]");                                  // start the replacement copy loop at the first replacement slot
    emitter.instruction("cbz x2, __rt_asiu_done");                              // a null replacement inserts nothing
    emitter.instruction("ldr x9, [x2]");                                        // x9 = replacement element count
    emitter.instruction("cbz x9, __rt_asiu_done");                              // an empty replacement inserts nothing

    emit_open_word_gap(emitter, "__rt_asiu");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asiu_copy");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("cmp x4, x9");                                          // has every replacement element been copied?
    emitter.instruction("b.ge __rt_asiu_set_len");                              // yes, publish the extended destination length
    emitter.instruction("add x5, x2, #24");                                     // compute the replacement payload base address
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load the borrowed replacement payload
    emitter.instruction("mov x0, x6");                                          // move the boxed Mixed cell into the unbox argument register
    crate::abi::emit_call_label(emitter, convert);                              // read the cell's integer payload for the typed slot
    emitter.instruction("mov x6, x0");                                          // the plain integer is what the destination slot receives
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index after the unbox call
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the clamped insertion index
    emitter.instruction("add x3, x0, #24");                                     // compute the destination payload base address
    emitter.instruction("add x7, x1, x4");                                      // compute the destination slot for this replacement element
    emitter.instruction("str x6, [x3, x7, lsl #3]");                            // store the replacement payload into the opened gap
    emitter.instruction("add x4, x4, #1");                                      // advance to the next replacement element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the updated replacement copy index
    emitter.instruction("b __rt_asiu_copy");                                    // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asiu_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("ldr x10, [x0]");                                       // reload the previous destination logical length
    emitter.instruction("add x10, x10, x9");                                    // extend it by the inserted element count
    emitter.instruction("str x10, [x0]");                                       // persist the extended destination logical length

    emitter.label("__rt_asiu_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the possibly-relocated destination pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the insertion bookkeeping slots
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 System V variant of `__rt_array_splice_insert`.
fn emit_array_splice_insert_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert ---");
    emitter.label_global("__rt_array_splice_insert");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the insertion spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the insertion bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for destination, replacement, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested insertion index across growth
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the replacement copy loop at the first replacement slot
    emitter.instruction("test rdx, rdx");                                       // did the caller pass a replacement indexed array at all?
    emitter.instruction("jz __rt_asi_done_x86");                                // a null replacement inserts nothing
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // load the replacement element count
    emitter.instruction("test r8, r8");                                         // does the replacement contribute any elements?
    emitter.instruction("jz __rt_asi_done_x86");                                // an empty replacement inserts nothing

    // -- propagate the replacement's value_type so an empty destination is tagged --
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load the destination packed array kind word
    emitter.instruction("mov r10, QWORD PTR [rdx - 8]");                        // load the replacement packed array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // materialize the destination preservation mask
    emitter.instruction("and r9, r11");                                         // keep the heap marker, kind byte, and persistent COW bit
    emitter.instruction("and r10, 0x7f00");                                     // keep only the replacement value_type lane
    emitter.instruction("or r9, r10");                                          // combine the destination kind bits with the replacement tag
    emitter.instruction("mov QWORD PTR [rdi - 8], r9");                         // persist the propagated packed array value_type tag

    emit_open_word_gap(emitter, "__rt_asi");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asi_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("cmp rcx, r8");                                         // has every replacement element been copied?
    emitter.instruction("jge __rt_asi_set_len_x86");                            // yes, publish the extended destination length
    emitter.instruction("lea r9, [rdx + 24]");                                  // compute the replacement payload base address
    emitter.instruction("mov r10, QWORD PTR [r9 + rcx * 8]");                   // load the borrowed replacement payload
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the clamped insertion index
    emitter.instruction("lea r11, [rax + 24]");                                 // compute the destination payload base address
    emitter.instruction("lea rsi, [rsi + rcx]");                                // compute the destination slot for this replacement element
    emitter.instruction("mov QWORD PTR [r11 + rsi * 8], r10");                  // store the replacement payload into the opened gap
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated replacement copy index
    emitter.instruction("jmp __rt_asi_copy_x86");                               // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asi_set_len_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination logical length
    emitter.instruction("add r9, r8");                                          // extend it by the inserted element count
    emitter.instruction("mov QWORD PTR [rax], r9");                             // persist the extended destination logical length

    emitter.label("__rt_asi_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly-relocated destination pointer
    emitter.instruction("add rsp, 64");                                         // release the insertion bookkeeping spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 System V variant of `__rt_array_splice_insert_refcounted`.
fn emit_array_splice_insert_refcounted_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_refcounted ---");
    emitter.label_global("__rt_array_splice_insert_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the insertion spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the insertion bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for destination, replacement, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested insertion index across growth
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the replacement copy loop at the first replacement slot
    emitter.instruction("test rdx, rdx");                                       // did the caller pass a replacement indexed array at all?
    emitter.instruction("jz __rt_asir_done_x86");                               // a null replacement inserts nothing
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // load the replacement element count
    emitter.instruction("test r8, r8");                                         // does the replacement contribute any elements?
    emitter.instruction("jz __rt_asir_done_x86");                               // an empty replacement inserts nothing

    // -- propagate the replacement's value_type so an empty destination is tagged --
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load the destination packed array kind word
    emitter.instruction("mov r10, QWORD PTR [rdx - 8]");                        // load the replacement packed array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // materialize the destination preservation mask
    emitter.instruction("and r9, r11");                                         // keep the heap marker, kind byte, and persistent COW bit
    emitter.instruction("and r10, 0x7f00");                                     // keep only the replacement value_type lane
    emitter.instruction("or r9, r10");                                          // combine the destination kind bits with the replacement tag
    emitter.instruction("mov QWORD PTR [rdi - 8], r9");                         // persist the propagated packed array value_type tag

    emit_open_word_gap(emitter, "__rt_asir");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asir_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("cmp rcx, r8");                                         // has every replacement element been copied?
    emitter.instruction("jge __rt_asir_set_len_x86");                           // yes, publish the extended destination length
    emitter.instruction("lea r9, [rdx + 24]");                                  // compute the replacement payload base address
    emitter.instruction("mov r10, QWORD PTR [r9 + rcx * 8]");                   // load the borrowed replacement payload
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the borrowed payload across the retain call
    emitter.instruction("mov rax, r10");                                        // move the borrowed payload into the retain argument register
    emitter.instruction("call __rt_incref");                                    // retain it before the destination array becomes an owner
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the retained payload after the retain call
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index after the retain call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the clamped insertion index
    emitter.instruction("lea r11, [rax + 24]");                                 // compute the destination payload base address
    emitter.instruction("lea rsi, [rsi + rcx]");                                // compute the destination slot for this replacement element
    emitter.instruction("mov QWORD PTR [r11 + rsi * 8], r10");                  // store the replacement payload into the opened gap
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated replacement copy index
    emitter.instruction("jmp __rt_asir_copy_x86");                              // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asir_set_len_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination logical length
    emitter.instruction("add r9, r8");                                          // extend it by the inserted element count
    emitter.instruction("mov QWORD PTR [rax], r9");                             // persist the extended destination logical length

    emitter.label("__rt_asir_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly-relocated destination pointer
    emitter.instruction("add rsp, 64");                                         // release the insertion bookkeeping spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 System V variant of `__rt_array_splice_insert_boxed`.
fn emit_array_splice_insert_boxed_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_boxed ---");
    emitter.label_global("__rt_array_splice_insert_boxed");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the insertion spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the insertion bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for destination, replacement, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested insertion index across growth
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the replacement copy loop at the first replacement slot
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // preserve the replacement slot value_type tag for the boxing calls
    emitter.instruction("test rdx, rdx");                                       // did the caller pass a replacement indexed array at all?
    emitter.instruction("jz __rt_asib_done_x86");                               // a null replacement inserts nothing
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // load the replacement element count
    emitter.instruction("test r8, r8");                                         // does the replacement contribute any elements?
    emitter.instruction("jz __rt_asib_done_x86");                               // an empty replacement inserts nothing

    // -- the destination stores boxed Mixed cells whatever the replacement slots hold --
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load the destination packed array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // materialize the destination preservation mask
    emitter.instruction("and r9, r11");                                         // keep the heap marker, kind byte, and persistent COW bit
    emitter.instruction("or r9, 0x700");                                        // runtime value_type tag 7 marks boxed Mixed payload slots
    emitter.instruction("mov QWORD PTR [rdi - 8], r9");                         // persist the boxed Mixed value_type tag

    emit_open_word_gap(emitter, "__rt_asib");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asib_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("cmp rcx, r8");                                         // has every replacement element been copied?
    emitter.instruction("jge __rt_asib_set_len_x86");                           // yes, publish the extended destination length
    emitter.instruction("lea r9, [rdx + 24]");                                  // compute the replacement payload base address
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the replacement slot value_type tag
    emitter.instruction("cmp rax, 1");                                          // do the replacement slots hold string pointer/length pairs?
    emitter.instruction("je __rt_asib_copy_string_x86");                        // string slots need a wider load
    emitter.instruction("mov rdi, QWORD PTR [r9 + rcx * 8]");                   // load the raw replacement payload from its 8-byte slot
    emitter.instruction("xor esi, esi");                                        // scalar Mixed payloads leave the high payload word clear
    emitter.instruction("jmp __rt_asib_copy_box_x86");                          // the payload words are ready for the Mixed cell allocator
    emitter.label("__rt_asib_copy_string_x86");
    emitter.instruction("mov r11, rcx");                                        // seed the byte offset computation from the replacement index
    emitter.instruction("shl r11, 4");                                          // each string slot is a 16-byte pointer/length pair
    emitter.instruction("add r9, r11");                                         // advance to this element's string slot
    emitter.instruction("mov rdi, QWORD PTR [r9]");                             // load the borrowed string pointer from the replacement slot
    emitter.instruction("mov rsi, QWORD PTR [r9 + 8]");                         // load the borrowed string length from the replacement slot
    emitter.label("__rt_asib_copy_box_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // pass the replacement slot value_type tag to the boxer
    emitter.instruction("call __rt_mixed_from_value");                          // allocate one owned Mixed cell for this element
    emitter.instruction("mov r10, rax");                                        // the fresh Mixed cell is what the destination slot receives
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index after the boxing call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the clamped insertion index
    emitter.instruction("lea r11, [rax + 24]");                                 // compute the destination payload base address
    emitter.instruction("lea rsi, [rsi + rcx]");                                // compute the destination slot for this replacement element
    emitter.instruction("mov QWORD PTR [r11 + rsi * 8], r10");                  // store the replacement payload into the opened gap
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated replacement copy index
    emitter.instruction("jmp __rt_asib_copy_x86");                              // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asib_set_len_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination logical length
    emitter.instruction("add r9, r8");                                          // extend it by the inserted element count
    emitter.instruction("mov QWORD PTR [rax], r9");                             // persist the extended destination logical length

    emitter.label("__rt_asib_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly-relocated destination pointer
    emitter.instruction("add rsp, 64");                                         // release the insertion bookkeeping spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 System V variant of `__rt_array_splice_insert_unboxed`.
fn emit_array_splice_insert_unboxed_x86_64(emitter: &mut impl Emitter, convert: &str) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_unboxed ---");
    emitter.label_global("__rt_array_splice_insert_unboxed");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the insertion spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the insertion bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for destination, replacement, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested insertion index across growth
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the replacement copy loop at the first replacement slot
    emitter.instruction("test rdx, rdx");                                       // did the caller pass a replacement indexed array at all?
    emitter.instruction("jz __rt_asiu_done_x86");                               // a null replacement inserts nothing
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // load the replacement element count
    emitter.instruction("test r8, r8");                                         // does the replacement contribute any elements?
    emitter.instruction("jz __rt_asiu_done_x86");                               // an empty replacement inserts nothing

    emit_open_word_gap(emitter, "__rt_asiu");

    // -- copy the replacement payloads into the opened gap --
    emitter.label("__rt_asiu_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("cmp rcx, r8");                                         // has every replacement element been copied?
    emitter.instruction("jge __rt_asiu_set_len_x86");                           // yes, publish the extended destination length
    emitter.instruction("lea r9, [rdx + 24]");                                  // compute the replacement payload base address
    emitter.instruction("mov r10, QWORD PTR [r9 + rcx * 8]");                   // load the borrowed replacement payload
    emitter.instruction("mov rax, r10");                                        // move the boxed Mixed cell into the x86_64 unbox input register
    crate::abi::emit_call_label(emitter, convert);                            // read the cell's integer payload for the typed slot
    emitter.instruction("mov r10, rax");                                        // the plain integer is what the destination slot receives
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index after the unbox call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the clamped insertion index
    emitter.instruction("lea r11, [rax + 24]");                                 // compute the destination payload base address
    emitter.instruction("lea rsi, [rsi + rcx]");                                // compute the destination slot for this replacement element
    emitter.instruction("mov QWORD PTR [r11 + rsi * 8], r10");                  // store the replacement payload into the opened gap
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated replacement copy index
    emitter.instruction("jmp __rt_asiu_copy_x86");                              // continue copying replacement elements

    // -- publish the extended destination length --
    emitter.label("__rt_asiu_set_len_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r8, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload the previous destination logical length
    emitter.instruction("add r9, r8");                                          // extend it by the inserted element count
    emitter.instruction("mov QWORD PTR [rax], r9");                             // persist the extended destination logical length

    emitter.label("__rt_asiu_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly-relocated destination pointer
    emitter.instruction("add rsp, 64");                                         // release the insertion bookkeeping spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the `__rt_array_splice_insert_str` runtime helper for the active target.
///
/// Writes `$replacement`'s strings into the 16-byte-slot gap the removal opened, duplicating each
/// payload with `__rt_str_persist` so the destination and the replacement array each own their own
/// bytes.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = destination indexed array, `x1` = insertion index, `x2` = replacement
///   indexed array (0 inserts nothing)
/// - **Output**: `x0` = the possibly-relocated destination indexed array
///
/// ## x86_64 ABI
/// - **Input**: `rdi` = destination, `rsi` = insertion index, `rdx` = replacement
/// - **Output**: `rax` = the possibly-relocated destination indexed array
///
/// A destination that is still empty carries the 8-byte `array<never>` shape its literal was
/// allocated with, so the first write re-scales the capacity into 16-byte slots and stamps the
/// string `value_type` exactly like `__rt_array_push_str` does. The insertion index is clamped to
/// `[0, length]`, so an unnormalized offset still cannot write outside the payload.
pub fn emit_array_splice_insert_str(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_array_splice_insert_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_str ---");
    emitter.label_global("__rt_array_splice_insert_str");

    // Stack layout: [sp,#0] destination array, [sp,#8] replacement array,
    //               [sp,#16] insertion index, [sp,#24] copy loop index, [sp,#48] saved x29/x30.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the insertion bookkeeping slots plus the saved frame registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the destination indexed-array pointer across growth
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the replacement indexed-array pointer across growth
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the requested insertion index across growth
    emitter.instruction("str xzr, [sp, #24]");                                  // start the replacement copy loop at the first replacement slot
    emitter.instruction("cbz x2, __rt_asis_done");                              // a null replacement inserts nothing
    emitter.instruction("ldr x9, [x2]");                                        // x9 = replacement element count
    emitter.instruction("cbz x9, __rt_asis_done");                              // an empty replacement inserts nothing

    // -- specialize a still-empty destination to 16-byte string slots before the first write --
    emitter.instruction("ldr x10, [x0]");                                       // load the destination logical length
    emitter.instruction("cbnz x10, __rt_asis_shape_ready");                     // a non-empty destination already has its string shape fixed
    emitter.instruction("ldr x11, [x0, #16]");                                  // x11 = old elem_size (8 for an empty array<never> buffer)
    emitter.instruction("ldr x12, [x0, #8]");                                   // x12 = old capacity counted in old-elem_size slots
    emitter.instruction("mul x12, x12, x11");                                   // x12 = backing-store data bytes already reserved
    emitter.instruction("lsr x12, x12, #4");                                    // reinterpret the same bytes as 16-byte string slots
    emitter.instruction("str x12, [x0, #8]");                                   // publish slot-accurate capacity before any 16-byte write
    emitter.instruction("mov x11, #16");                                        // string payload slots carry a pointer and a length
    emitter.instruction("str x11, [x0, #16]");                                  // elem_size = 16 before any future grow copies live string slots
    emitter.label("__rt_asis_shape_ready");

    // -- the destination stores string pointer/length pairs whatever it held before --
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the destination kind byte and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // drop stale destination value_type bits before restamping
    emitter.instruction("mov x11, #0x100");                                     // runtime value_type tag 1 marks string payload slots
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination kind bits with the string value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the string value_type tag

    emit_open_string_gap(emitter);

    // -- duplicate the replacement strings into the opened gap --
    emitter.label("__rt_asis_copy");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("cmp x4, x9");                                          // has every replacement string been copied?
    emitter.instruction("b.ge __rt_asis_set_len");                              // yes, publish the extended destination length
    emitter.instruction("add x5, x2, #24");                                     // compute the replacement string payload base address
    emitter.instruction("lsl x6, x4, #4");                                      // scale the copy index by the 16-byte string slot size
    emitter.instruction("add x5, x5, x6");                                      // compute this element's replacement string slot address
    emitter.instruction("ldr x1, [x5]");                                        // load the borrowed replacement string pointer
    emitter.instruction("ldr x2, [x5, #8]");                                    // load the borrowed replacement string length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate it so the destination owns its own bytes
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the replacement copy index after the persist call
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the clamped insertion index
    emitter.instruction("add x5, x0, #24");                                     // compute the destination string payload base address
    emitter.instruction("add x6, x3, x4");                                      // compute the destination slot for this replacement element
    emitter.instruction("lsl x6, x6, #4");                                      // scale that slot index by the string slot size
    emitter.instruction("add x6, x5, x6");                                      // compute the destination string slot address
    emitter.instruction("str x1, [x6]");                                        // store the owned string pointer into the opened gap
    emitter.instruction("str x2, [x6, #8]");                                    // store the matching owned string length
    emitter.instruction("add x4, x4, #1");                                      // advance to the next replacement element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the updated replacement copy index
    emitter.instruction("b __rt_asis_copy");                                    // continue copying replacement strings

    // -- publish the extended destination length --
    emitter.label("__rt_asis_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination indexed-array pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the replacement indexed-array pointer
    emitter.instruction("ldr x9, [x2]");                                        // reload the replacement element count
    emitter.instruction("ldr x10, [x0]");                                       // reload the previous destination logical length
    emitter.instruction("add x10, x10, x9");                                    // extend it by the inserted element count
    emitter.instruction("str x10, [x0]");                                       // persist the extended destination logical length

    emitter.label("__rt_asis_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the possibly-relocated destination pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the insertion bookkeeping slots
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 System V variant of `__rt_array_splice_insert_str`.
///
/// Mirrors the ARM64 sequence; only the register encoding differs. Note that `__rt_str_persist`
/// takes its source pointer in `rax` (not the first SysV argument register) and returns the owned
/// pointer in `rax` with the length in `rdx`.
fn emit_array_splice_insert_str_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_insert_str ---");
    emitter.label_global("__rt_array_splice_insert_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the insertion spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the insertion bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for destination, replacement, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the replacement indexed-array pointer across growth
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested insertion index across growth
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the replacement copy loop at the first replacement slot
    emitter.instruction("test rdx, rdx");                                       // is the replacement pointer null?
    emitter.instruction("jz __rt_asis_done_x86");                               // a null replacement inserts nothing
    emitter.instruction("mov r9, QWORD PTR [rdx]");                             // r9 = replacement element count
    emitter.instruction("test r9, r9");                                         // is the replacement array empty?
    emitter.instruction("jz __rt_asis_done_x86");                               // an empty replacement inserts nothing

    // -- specialize a still-empty destination to 16-byte string slots before the first write --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the destination logical length
    emitter.instruction("test r10, r10");                                       // is this the first write into a still-empty destination?
    emitter.instruction("jnz __rt_asis_shape_ready_x86");                       // a non-empty destination already has its string shape fixed
    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // r10 = old elem_size (8 for an empty array<never> buffer)
    emitter.instruction("mov r11, QWORD PTR [rdi + 8]");                        // r11 = old capacity counted in old-elem_size slots
    emitter.instruction("imul r11, r10");                                       // r11 = backing-store data bytes already reserved
    emitter.instruction("shr r11, 4");                                          // reinterpret the same bytes as 16-byte string slots
    emitter.instruction("mov QWORD PTR [rdi + 8], r11");                        // publish slot-accurate capacity before any 16-byte write
    emitter.instruction("mov QWORD PTR [rdi + 16], 16");                        // elem_size = 16 before any future grow copies live string slots
    emitter.label("__rt_asis_shape_ready_x86");

    // -- the destination stores string pointer/length pairs whatever it held before --
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the destination packed array kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed-array kind, and persistent COW metadata
    emitter.instruction("and r10, r11");                                        // drop stale destination value_type bits before restamping
    emitter.instruction("or r10, 0x100");                                       // runtime value_type tag 1 marks string payload slots
    emitter.instruction("mov QWORD PTR [rdi - 8], r10");                        // persist the string value_type tag

    emit_open_string_gap(emitter);

    // -- duplicate the replacement strings into the opened gap --
    emitter.label("__rt_asis_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("cmp rcx, r9");                                         // has every replacement string been copied?
    emitter.instruction("jge __rt_asis_set_len_x86");                           // yes, publish the extended destination length
    emitter.instruction("lea r10, [rdx + 24]");                                 // compute the replacement string payload base address
    emitter.instruction("mov rax, rcx");                                        // copy the replacement copy index before scaling it
    emitter.instruction("shl rax, 4");                                          // scale it by the 16-byte string slot size
    emitter.instruction("mov r11, QWORD PTR [r10 + rax]");                      // load the borrowed replacement string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + rax + 8]");                  // load the borrowed replacement string length
    emitter.instruction("mov rax, r11");                                        // __rt_str_persist reads its source pointer from rax, not rdi
    emitter.instruction("call __rt_str_persist");                               // duplicate it so the destination owns its own bytes
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the replacement copy index after the persist call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the clamped insertion index
    emitter.instruction("lea r10, [rdi + 24]");                                 // compute the destination string payload base address
    emitter.instruction("mov r11, rsi");                                        // copy the insertion index before computing the target slot
    emitter.instruction("add r11, rcx");                                        // compute the destination slot for this replacement element
    emitter.instruction("shl r11, 4");                                          // scale that slot index by the string slot size
    emitter.instruction("mov QWORD PTR [r10 + r11], rax");                      // store the owned string pointer into the opened gap
    emitter.instruction("mov QWORD PTR [r10 + r11 + 8], rdx");                  // store the matching owned string length
    emitter.instruction("add rcx, 1");                                          // advance to the next replacement element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated replacement copy index
    emitter.instruction("jmp __rt_asis_copy_x86");                              // continue copying replacement strings

    // -- publish the extended destination length --
    emitter.label("__rt_asis_set_len_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the destination indexed-array pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the replacement indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rdx]");                             // reload the replacement element count
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // reload the previous destination logical length
    emitter.instruction("add r10, r9");                                         // extend it by the inserted element count
    emitter.instruction("mov QWORD PTR [rdi], r10");                            // persist the extended destination logical length

    emitter.label("__rt_asis_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the possibly-relocated destination pointer
    emitter.instruction("add rsp, 64");                                         // release the insertion bookkeeping slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
