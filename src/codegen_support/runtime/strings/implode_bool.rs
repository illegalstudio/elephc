//! Purpose:
//! Emits the `__rt_implode_bool` runtime helper assembly for `implode()` over an indexed array
//! whose elements are statically typed `bool` (or the `false` literal subtype).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - PHP stringifies `true` as `"1"` and `false` as the EMPTY string, not as `"0"`. The
//!   integer-optimized `__rt_implode_int` helper runs every element through `__rt_itoa`, which
//!   renders a false payload as `"0"` — so a bool-element array must not use it. This helper is
//!   the bool-element counterpart: it appends the single byte `'1'` for a non-zero payload and
//!   appends nothing at all for a zero payload.
//! - The glue is still emitted between EVERY pair of elements, including around empty (false)
//!   renderings, matching PHP (`implode(",", [true, false])` is `"1,"`).
//! - No `__rt_itoa` call is made, so this helper never advances the shared concat scratch behind
//!   the caller's back: it only appends into the implode result region of `_concat_buf` and
//!   stamps `_concat_off` once at the end.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_implode_bool` runtime helper for PHP `implode()` with bool array elements.
///
/// Dispatches to the x86_64 variant; ARM64 is emitted inline below.
/// ABI (ARM64): x1/x2 = glue_ptr/glue_len, x3 = array_ptr → x1 = result_ptr, x2 = result_len.
/// ABI (x86_64): rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
/// Uses the shared concat buffer and advances `_concat_off` by the bytes written.
pub fn emit_implode_bool(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_bool_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode_bool ---");
    emitter.label_global("__rt_implode_bool");

    // -- set up stack frame (48 bytes; no nested calls, so no lr save is needed) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate scratch slots for glue, array, and cursors
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save the source indexed-array pointer

    // -- get concat_buf write position --
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "x8");
    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "x7");
    emitter.instruction("add x9, x7, x8");                                      // compute the implode destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save the result start pointer
    emitter.instruction("str x6, [sp, #32]");                                   // save the concat offset variable address

    // -- load array length and initialize index --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the source indexed-array pointer
    emitter.instruction("ldr x10, [x3]");                                       // load the source element count
    emitter.instruction("mov x11, #0");                                         // initialize the element index to zero

    emitter.label("__rt_implode_bool_loop");
    emitter.instruction("cmp x11, x10");                                        // check whether every bool element was rendered
    emitter.instruction("b.ge __rt_implode_bool_done");                         // finalize once the source array is exhausted

    // -- insert glue before element (skipped for the first element) --
    emitter.instruction("cbz x11, __rt_implode_bool_elem");                     // no glue precedes the first element
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    emitter.instruction("mov x12, x2");                                         // copy the glue length as the byte counter
    emitter.label("__rt_implode_bool_glue");
    emitter.instruction("cbz x12, __rt_implode_bool_elem");                     // render the element once the glue is fully copied
    emitter.instruction("ldrb w13, [x1], #1");                                  // load one glue byte and advance the source pointer
    emitter.instruction("strb w13, [x9], #1");                                  // store one glue byte and advance the destination pointer
    emitter.instruction("sub x12, x12, #1");                                    // decrement the remaining glue byte count
    emitter.instruction("b __rt_implode_bool_glue");                            // continue copying glue bytes

    // -- render the current bool element: "1" when true, nothing when false --
    emitter.label("__rt_implode_bool_elem");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the source indexed-array pointer
    emitter.instruction("add x3, x3, #24");                                     // skip the 24-byte array header to reach the payload region
    emitter.instruction("ldr x14, [x3, x11, lsl #3]");                          // load the 8-byte bool payload at the current index
    emitter.instruction("cbz x14, __rt_implode_bool_next");                     // PHP renders false as the empty string: append nothing
    emitter.instruction("mov w13, #49");                                        // ASCII '1' is the entire rendering of PHP true
    emitter.instruction("strb w13, [x9], #1");                                  // append the single true byte and advance the destination pointer

    emitter.label("__rt_implode_bool_next");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next bool element
    emitter.instruction("b __rt_implode_bool_loop");                            // continue joining bool elements

    // -- finalize: compute the result length and publish the new concat offset --
    emitter.label("__rt_implode_bool_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load the result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "x8"); // load the current concat offset (ctx-relative in ctx mode)
    emitter.instruction("add x8, x8, x2");                                      // advance the offset by the joined result length
    crate::codegen_support::runtime::ctx::emit_concat_off_store(emitter, "x8"); // publish the updated concat offset (ctx-relative in ctx mode)
    emitter.instruction("add sp, sp, #48");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the joined string in x1/x2
}

/// Emits `__rt_implode_bool` for Linux x86_64.
///
/// ABI: rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
/// Mirrors the ARM64 rendering rule exactly: `'1'` for a non-zero payload, nothing for zero.
fn emit_implode_bool_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode_bool ---");
    emitter.label_global("__rt_implode_bool");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving bool-implode spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the glue, array, and cursor slots
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for glue, array, destination cursor, length, and index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the glue string pointer for every separator emission
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the glue string length for every separator emission
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the source indexed-array pointer across the render loop
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "r9");
    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "r10");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute the bool-implode destination pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the result start pointer for the final length computation
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the running destination cursor across glue and element emission
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the source indexed-array logical length once
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the source length for the loop termination check
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the element cursor to the first bool element

    emitter.label("__rt_implode_bool_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current element cursor
    emitter.instruction("cmp r11, QWORD PTR [rbp - 48]");                       // compare the cursor against the saved source length
    emitter.instruction("jae __rt_implode_bool_done");                          // finalize once every bool element has been rendered
    emitter.instruction("test r11, r11");                                       // check whether this is the first element
    emitter.instruction("jz __rt_implode_bool_elem");                           // no glue precedes the first element
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the glue string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the glue string length
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the running destination cursor

    emitter.label("__rt_implode_bool_glue");
    emitter.instruction("test r9, r9");                                         // check whether every glue byte has been copied
    emitter.instruction("jz __rt_implode_bool_glue_done");                      // render the element once the glue is exhausted
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one glue byte
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one glue byte into the concat buffer
    emitter.instruction("add r8, 1");                                           // advance the glue source pointer
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer
    emitter.instruction("sub r9, 1");                                           // decrement the remaining glue byte count
    emitter.instruction("jmp __rt_implode_bool_glue");                          // continue copying glue bytes

    emitter.label("__rt_implode_bool_glue_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the destination cursor after the separator bytes

    emitter.label("__rt_implode_bool_elem");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current element cursor
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the source indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [r8 + r11 * 8 + 24]");               // load the 8-byte bool payload at the current index
    emitter.instruction("test r9, r9");                                         // check whether the payload is PHP false
    emitter.instruction("jz __rt_implode_bool_next");                           // PHP renders false as the empty string: append nothing
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the running destination cursor
    emitter.instruction("mov BYTE PTR [r10], 49");                              // ASCII '1' is the entire rendering of PHP true
    emitter.instruction("add r10, 1");                                          // advance past the appended true byte
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the destination cursor after the true byte

    emitter.label("__rt_implode_bool_next");
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // advance to the next bool element
    emitter.instruction("jmp __rt_implode_bool_loop");                          // continue joining bool elements

    emitter.label("__rt_implode_bool_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final destination cursor
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the result start pointer
    emitter.instruction("mov rdx, r10");                                        // copy the final cursor before subtracting the start pointer
    emitter.instruction("sub rdx, rax");                                        // result length = dest_end - dest_start
    crate::codegen_support::runtime::ctx::emit_concat_off_load(emitter, "r9");
    emitter.instruction("add r9, rdx");                                         // advance the offset by the joined result length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the updated concat-buffer write offset
    emitter.instruction("add rsp, 64");                                         // release the bool-implode spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the joined string in rax/rdx
}
