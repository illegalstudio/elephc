//! Purpose:
//! Emits the `__rt_ref_marker_store` runtime helper: a plain assignment to an array element that
//! is a PHP reference writes THROUGH the reference instead of replacing it.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `__rt_hash_set` and `__rt_array_set_mixed`, when the slot being overwritten holds a marker.
//!
//! Key details:
//! - A marker box is `{tag 12|13, cell, source_tag}`. The helper handles the one cell shape an
//!   element reference creates, a cell holding a boxed Mixed handle (`source_tag` 7): the
//!   incoming value becomes a Mixed box, is installed in the cell, and the box it replaces is
//!   released. Every alias of the cell -- the bound local, other arrays sharing the marker --
//!   then reads the new value.
//! - The incoming value is CONSUMED, as it would have been by the slot write. A non-Mixed value
//!   is boxed with `__rt_mixed_from_value`, which retains its payload, so the caller's own
//!   reference to that payload is dropped afterwards.
//! - A typed cell is left alone and reported as unhandled (`0`), so the caller keeps its own
//!   replacement path; nothing is consumed in that case.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_ref_marker_store` for the current target.
///
/// Input:  x0/rdi = marker box, x1/rsi = value_lo, x2/rdx = value_hi, x3/rcx = value tag
/// Output: x0/rax = 1 when the value was written through the reference, 0 when the cell is typed
pub fn emit_ref_marker_store(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ref_marker_store_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ref_marker_store ---");
    emitter.label_global("__rt_ref_marker_store");

    emitter.instruction("ldr x9, [x0, #16]");                                   // load the runtime tag of the value the cell stores
    emitter.instruction("cmp x9, #7");                                          // only cells holding a boxed Mixed handle are written here
    emitter.instruction("b.ne __rt_ref_marker_store_typed");                    // a typed cell stays with the caller's replacement path

    // Frame: [sp,#0]=cell, [sp,#8]=value_lo, [sp,#16]=value tag, [sp,#24]=box to install.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the shared reference cell from the marker
    emitter.instruction("str x9, [sp, #0]");                                    // keep the cell across the boxing and release calls
    emitter.instruction("cmp x3, #7");                                          // is the incoming value already a boxed Mixed handle?
    emitter.instruction("b.eq __rt_ref_marker_store_have_box");                 // an owned box is installed as it stands
    emitter.instruction("str x1, [sp, #8]");                                    // keep the consumed payload to drop the caller's reference
    emitter.instruction("str x3, [sp, #16]");                                   // keep the consumed payload's tag
    emitter.instruction("mov x0, x3");                                          // box the incoming value under its own tag
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = a new box that retains the payload
    emitter.instruction("str x0, [sp, #24]");                                   // keep the new box across the payload release
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the consumed payload's tag
    emitter.instruction("cmp x9, #1");                                          // strings are refcounted heap payloads
    emitter.instruction("b.eq __rt_ref_marker_store_drop");                     // drop the caller's string reference
    emitter.instruction("cmp x9, #4");                                          // arrays, hashes and objects occupy tags 4 through 6
    emitter.instruction("b.lo __rt_ref_marker_store_boxed");                    // scalars own nothing
    emitter.instruction("cmp x9, #6");                                          // is the payload a heap container or object?
    emitter.instruction("b.hi __rt_ref_marker_store_boxed");                    // other tags keep their payload with the box
    emitter.label("__rt_ref_marker_store_drop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the consumed payload the box now retains
    emitter.instruction("bl __rt_decref_any");                                  // drop the reference the caller handed over
    emitter.label("__rt_ref_marker_store_boxed");
    emitter.instruction("ldr x1, [sp, #24]");                                   // the box to install
    emitter.label("__rt_ref_marker_store_have_box");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the shared reference cell
    emitter.instruction("ldr x0, [x9]");                                        // the box the reference held until now
    emitter.instruction("str x1, [x9]");                                        // every alias of the cell now reads the new value
    emitter.instruction("bl __rt_decref_mixed");                                // release the replaced box (null-safe)
    emitter.instruction("mov x0, #1");                                          // report the write as handled
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the slot writer

    emitter.label("__rt_ref_marker_store_typed");
    emitter.instruction("mov x0, #0");                                          // report a typed cell: nothing was consumed
    emitter.instruction("ret");                                                 // return to the slot writer
}

/// x86_64 variant of `__rt_ref_marker_store`.
fn emit_ref_marker_store_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ref_marker_store ---");
    emitter.label_global("__rt_ref_marker_store");

    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // load the runtime tag of the value the cell stores
    emitter.instruction("cmp r10, 7");                                          // only cells holding a boxed Mixed handle are written here
    emitter.instruction("jne __rt_ref_marker_store_typed");                     // a typed cell stays with the caller's replacement path

    // Frame: [rbp-8]=cell, [rbp-16]=value_lo, [rbp-24]=value tag, [rbp-32]=box to install.
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the helper frame, keeping calls 16-byte aligned
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the shared reference cell from the marker
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // keep the cell across the boxing and release calls
    emitter.instruction("mov r11, rsi");                                        // the incoming box, when the value is already Mixed
    emitter.instruction("cmp rcx, 7");                                          // is the incoming value already a boxed Mixed handle?
    emitter.instruction("je __rt_ref_marker_store_have_box");                   // an owned box is installed as it stands
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the consumed payload to drop the caller's reference
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // keep the consumed payload's tag
    emitter.instruction("mov rax, rcx");                                        // box the incoming value under its own tag
    emitter.instruction("mov rdi, rsi");                                        // the payload low word
    emitter.instruction("mov rsi, rdx");                                        // the payload high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = a new box that retains the payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the new box across the payload release
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the consumed payload's tag
    emitter.instruction("cmp r10, 1");                                          // strings are refcounted heap payloads
    emitter.instruction("je __rt_ref_marker_store_drop");                       // drop the caller's string reference
    emitter.instruction("cmp r10, 4");                                          // arrays, hashes and objects occupy tags 4 through 6
    emitter.instruction("jb __rt_ref_marker_store_boxed");                      // scalars own nothing
    emitter.instruction("cmp r10, 6");                                          // is the payload a heap container or object?
    emitter.instruction("ja __rt_ref_marker_store_boxed");                      // other tags keep their payload with the box
    emitter.label("__rt_ref_marker_store_drop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the consumed payload the box now retains
    emitter.instruction("call __rt_decref_any");                                // drop the reference the caller handed over
    emitter.label("__rt_ref_marker_store_boxed");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // the box to install
    emitter.label("__rt_ref_marker_store_have_box");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the shared reference cell
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // the box the reference held until now
    emitter.instruction("mov QWORD PTR [r10], r11");                            // every alias of the cell now reads the new value
    emitter.instruction("call __rt_decref_mixed");                              // release the replaced box (null-safe)
    emitter.instruction("mov rax, 1");                                          // report the write as handled
    emitter.instruction("leave");                                               // release the helper frame
    emitter.instruction("ret");                                                 // return to the slot writer

    emitter.label("__rt_ref_marker_store_typed");
    emitter.instruction("xor eax, eax");                                        // report a typed cell: nothing was consumed
    emitter.instruction("ret");                                                 // return to the slot writer
}
