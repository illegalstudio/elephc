//! Purpose:
//! Emits `__rt_array_splice_insert`, `__rt_array_splice_insert_refcounted`,
//! `__rt_array_splice_insert_boxed`, and `__rt_array_splice_insert_unboxed`, the runtime helpers
//! that write `array_splice()`'s `$replacement` values into the gap the removal opened.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The EIR lowering of `array_splice()` in `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - The destination is grown through `__rt_array_grow` BEFORE anything is written, so the
//!   right-slide and the replacement copy always stay inside the payload allocation. Growth may
//!   relocate the array, which is why the caller stores the returned pointer back into the
//!   by-reference receiver.
//! - The tail slide walks backwards, so a replacement longer than the removed window cannot
//!   overwrite an element it has not moved yet.
//! - The four variants differ only in what one replacement slot becomes in the destination:
//!   copied verbatim (scalar), retained first (refcounted, so the replacement array keeps its own
//!   reference), wrapped in a freshly allocated Mixed cell (boxed, for a heterogeneous
//!   destination fed a typed replacement), or read out of a Mixed cell as a plain integer
//!   (unboxed, for a typed destination fed a boxed replacement such as `[$x + 1]`).
//! - The insertion index is clamped to `[0, length]` in every variant, so a caller that hands
//!   over an unnormalized offset still cannot write outside the payload.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use elephc_memoir::values::array_splice_insert;

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
pub fn emit_array_splice_insert(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asi");

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
pub fn emit_array_splice_insert_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asir");

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
/// Takes one extra argument — the replacement's runtime value_type tag in `x3` / `rcx` — and
/// wraps every replacement payload in a fresh `__rt_mixed_from_value` cell, which the destination
/// then owns outright. That is what a heterogeneous `array<mixed>` receiver needs when the
/// replacement is a typed array such as `[7, 8, 9]` or `["a", "b"]`: the replacement keeps its raw
/// slots and is released by the caller's ordinary temporary cleanup. A string replacement
/// (`tag == 1`) reads 16-byte pointer/length slots and hands the borrowed pair to
/// `__rt_mixed_from_value`, which persists the bytes itself, so the Mixed cell never aliases
/// storage the replacement array still owns.
pub fn emit_array_splice_insert_boxed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asib");

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
/// is the shape an overflow-checked expression produces — `[$x + 1, $x + 2]` is an `array<mixed>`
/// because `ichecked_add` boxes its result — spliced into an `array<int>` receiver. Nothing is
/// retained or released: the destination stores a copy of the integer value, and the replacement
/// array keeps owning its cells.
///
/// `__rt_mixed_cast_int` reads its argument from `x0` / `rax` — it forwards straight into
/// `__rt_mixed_unbox`, whose x86_64 input register is `rax`, not the first SysV argument register.
pub fn emit_array_splice_insert_unboxed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_splice_insert_unboxed_x86_64(emitter);
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asiu");

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
    emitter.instruction("bl __rt_mixed_cast_int");                              // read the cell's integer payload for the typed slot
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
fn emit_array_splice_insert_x86_64(emitter: &mut Emitter) {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asi");

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
fn emit_array_splice_insert_refcounted_x86_64(emitter: &mut Emitter) {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asir");

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
fn emit_array_splice_insert_boxed_x86_64(emitter: &mut Emitter) {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asib");

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
fn emit_array_splice_insert_unboxed_x86_64(emitter: &mut Emitter) {
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

    array_splice_insert::emit_open_word_gap(emitter, "__rt_asiu");

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
    emitter.instruction("call __rt_mixed_cast_int");                            // read the cell's integer payload for the typed slot
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
