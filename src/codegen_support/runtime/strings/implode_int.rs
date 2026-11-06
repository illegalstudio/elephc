//! Purpose:
//! Emits the `__rt_implode_int`, `__rt_implode_int_loop` runtime helper assembly for implode int.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - Packed int joins whose worst-case output fits the remaining concat capacity take a
//!   register-state fast path with direct 0-99 formatting. Larger joins branch to the shared
//!   `__rt_implode_typed` body, which grows the destination onto the heap.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits runtime helper for PHP `implode()` with integer array elements.
/// Dispatches to x86_64 variant; ARM64 uses the main implementation below.
/// ABI: x1/x2=glue_ptr/glue_len, x3=array_ptr → x1=result_ptr, x2=result_len.
/// Uses the shared concat buffer; updates `_concat_off` to reflect written bytes.
/// Side effects: writes to concat_buf, advances concat_off global.
pub fn emit_implode_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode_int ---");
    emitter.label_global("__rt_implode_int");

    // -- frame: callee-saved loop state, a private 21-byte itoa window, and linkage --
    emitter.instruction("sub sp, sp, #160");                                    // reserve loop state, itoa window, and linkage
    emitter.instruction("stp x29, x30, [sp, #144]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #144");                                   // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // preserve the glue pair for the shared-join fallback
    emitter.instruction("str x3, [sp, #16]");                                   // preserve the array pointer for the shared-join fallback
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved destination and payload base
    emitter.instruction("stp x21, x22, [sp, #80]");                             // save callee-saved element count and index
    emitter.instruction("stp x23, x24, [sp, #96]");                             // save callee-saved glue pointer and length
    emitter.instruction("mov x23, x1");                                         // keep the glue pointer in a callee-saved register
    emitter.instruction("mov x24, x2");                                         // keep the glue length in a callee-saved register
    emitter.instruction("add x20, x3, #24");                                    // address the packed integer payload base
    emitter.instruction("ldr x21, [x3]");                                       // load the packed element count once
    emitter.instruction("mov x22, xzr");                                        // start at the first element
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x12", emitter.concat_capacity as i64);
    emitter.instruction("subs x12, x12, x10");                                  // room left in the shared concat buffer
    emitter.instruction("b.lo __rt_implode_int_fallback");                      // an over-full scratch join uses the growable body
    emitter.instruction("cmp x24, #64");                                        // bound the separator length used in the capacity check
    emitter.instruction("b.hi __rt_implode_int_fallback");                      // unusually long glue uses the growable body
    emitter.instruction("add x13, x24, #20");                                   // worst-case bytes per element: glue plus an int64
    emitter.instruction("mul x14, x21, x13");                                   // total worst-case output size
    emitter.instruction("umulh x15, x21, x13");                                 // detect a size multiplication overflow
    emitter.instruction("cbnz x15, __rt_implode_int_fallback");                 // an unrepresentable bound uses the growable body
    emitter.instruction("cmp x14, x12");                                        // does the worst-case output fit the remaining capacity?
    emitter.instruction("b.hi __rt_implode_int_fallback");                      // larger joins use the growable body
    emitter.instruction("str x9, [sp, #32]");                                   // save the offset variable address for finalization
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x19, x11, x10");                                   // compute the destination cursor
    emitter.instruction("str x19, [sp, #24]");                                  // save the result start pointer

    // -- join each integer, keeping the whole loop state in registers --
    emitter.label("__rt_implode_int_loop");
    emitter.instruction("cmp x22, x21");                                        // have all elements been processed?
    emitter.instruction("b.ge __rt_implode_int_done");                          // finalize the joined string
    emitter.instruction("cbz x22, __rt_implode_int_elem");                      // skip the glue before the first element
    emitter.instruction("cmp x24, #1");                                         // is the separator exactly one byte?
    emitter.instruction("b.ne __rt_implode_int_glue_loop");                     // other lengths keep the general copy loop
    emitter.instruction("ldrb w13, [x23]");                                     // load the single separator byte
    emitter.instruction("strb w13, [x19], #1");                                 // store it and advance the destination cursor
    emitter.instruction("b __rt_implode_int_elem");                             // continue with the integer element

    emitter.label("__rt_implode_int_glue_loop");
    emitter.instruction("mov x9, x23");                                         // start the glue copy at the separator pointer
    emitter.instruction("mov x12, x24");                                        // copy the separator length as the byte counter
    emitter.label("__rt_implode_int_glue_copy");
    emitter.instruction("cbz x12, __rt_implode_int_elem");                      // continue with the element once the glue is copied
    emitter.instruction("ldrb w13, [x9], #1");                                  // load one separator byte
    emitter.instruction("strb w13, [x19], #1");                                 // store it and advance the destination cursor
    emitter.instruction("sub x12, x12, #1");                                    // account for the copied separator byte
    emitter.instruction("b __rt_implode_int_glue_copy");                        // copy the remaining separator bytes

    emitter.label("__rt_implode_int_elem");
    emitter.instruction("ldr x0, [x20, x22, lsl #3]");                          // load the next packed integer element
    emitter.instruction("cmp x0, #100");                                        // do the common small values fit the direct formatter?
    emitter.instruction("b.hs __rt_implode_int_slow");                          // larger and negative values use the private itoa window
    emitter.instruction("cmp x0, #10");                                         // is the value a single decimal digit?
    emitter.instruction("b.hs __rt_implode_int_two");                           // two-digit values split with a reciprocal multiply
    emitter.instruction("add w13, w0, #48");                                    // convert the single digit to ASCII
    emitter.instruction("strb w13, [x19], #1");                                 // store the digit and advance the destination cursor
    emitter.instruction("b __rt_implode_int_next");                             // advance to the next array element

    emitter.label("__rt_implode_int_two");
    emitter.instruction("mov x12, #205");                                       // reciprocal numerator for v / 10 on 0..99
    emitter.instruction("mul x13, x0, x12");                                    // scale the value for the reciprocal division
    emitter.instruction("lsr x13, x13, #11");                                   // x13 = floor(v / 10) for v in 0..99
    emitter.instruction("mov x12, #10");                                        // decimal radix for the remainder
    emitter.instruction("msub x14, x13, x12, x0");                              // remainder is the ones digit
    emitter.instruction("add w13, w13, #48");                                   // convert the tens digit to ASCII
    emitter.instruction("add w14, w14, #48");                                   // convert the ones digit to ASCII
    emitter.instruction("strb w13, [x19], #1");                                 // store the tens digit
    emitter.instruction("strb w14, [x19], #1");                                 // store the ones digit
    emitter.instruction("b __rt_implode_int_next");                             // advance to the next array element

    emitter.label("__rt_implode_int_slow");
    emitter.instruction("add x1, sp, #112");                                    // point at the private 21-byte itoa window
    emitter.instruction("bl __rt_itoa_into");                                   // format without touching the shared concat buffer
    emitter.instruction("mov x12, x1");                                         // returned length becomes the copy counter
    emitter.instruction("mov x13, x0");                                         // returned pointer becomes the copy source
    emitter.label("__rt_implode_int_slow_copy");
    emitter.instruction("cbz x12, __rt_implode_int_next");                      // continue once every formatted byte is copied
    emitter.instruction("ldrb w14, [x13], #1");                                 // load one formatted byte
    emitter.instruction("strb w14, [x19], #1");                                 // store it and advance the destination cursor
    emitter.instruction("sub x12, x12, #1");                                    // account for the copied byte
    emitter.instruction("b __rt_implode_int_slow_copy");                        // copy the remaining formatted bytes

    emitter.label("__rt_implode_int_next");
    emitter.instruction("add x22, x22, #1");                                    // advance to the next array element
    emitter.instruction("b __rt_implode_int_loop");                             // process the remaining elements

    // -- finalize: compute the result length and advance concat_off once --
    emitter.label("__rt_implode_int_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the result start pointer
    emitter.instruction("sub x2, x19, x1");                                     // result length = destination end - start
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the offset variable address
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emitter.instruction("add x10, x10, x2");                                    // advance it by the joined string length
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat-buffer offset
    emitter.instruction("ldp x23, x24, [sp, #96]");                             // restore the glue pointer and length
    emitter.instruction("ldp x21, x22, [sp, #80]");                             // restore the element count and index
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore the destination and payload base
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // release the integer-implode frame
    emitter.instruction("ret");                                                 // return the joined string in x1/x2

    emitter.label("__rt_implode_int_fallback");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the glue pair for the shared join
    emitter.instruction("ldr x3, [sp, #16]");                                   // restore the packed array pointer
    emitter.instruction("mov x4, #0");                                          // force the raw-int element layout
    emitter.instruction("ldp x23, x24, [sp, #96]");                             // restore the callee-saved glue pair
    emitter.instruction("ldp x21, x22, [sp, #80]");                             // restore the callee-saved count and index
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore the callee-saved destination and payload base
    emitter.instruction("ldp x29, x30, [sp, #144]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // release the fast-path frame
    emitter.instruction("b __rt_implode_typed");                                // join through the bounded, growable body
}

/// Emits `__rt_implode_int` runtime helper for Linux x86_64.
/// ABI: rdi/rsi=glue_ptr/glue_len, rdx=array_ptr → rax=result_ptr, rdx=result_len.
/// Copies glue and each integer element (converted via `__rt_itoa`) into the shared concat buffer.
/// Advances `_concat_off` by the total bytes written.
/// Stack frame: 64 bytes of spill slots for glue, array, destination cursor, length, and loop index.
fn emit_implode_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode_int ---");
    emitter.label_global("__rt_implode_int");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving integer-implode spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for glue, array, concat-buffer bookkeeping, and the loop cursor
    emitter.instruction("sub rsp, 112");                                        // reserve spills, a 21-byte itoa window, and alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the glue string pointer across integer conversion and copy helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the glue string length across integer conversion and copy helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the indexed-array pointer across integer conversion and copy helper calls
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before materializing the implode output start pointer
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the element count for the capacity check
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // load the separator length for the capacity check
    emitter.instruction("cmp rcx, 64");                                         // bound the separator length used in the capacity check
    emitter.instruction("ja __rt_implode_int_fallback_x86");                    // unusually long glue uses the growable body
    emitter.instruction("add rcx, 20");                                         // worst-case bytes per element: glue plus an int64
    emitter.instruction("mov rax, r11");                                        // copy the element count for the bound multiplication
    emitter.instruction("imul rax, rcx");                                       // total worst-case output size
    emitter.instruction("jo __rt_implode_int_fallback_x86");                    // an unrepresentable bound uses the growable body
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rcx", emitter.concat_capacity as i64);
    emitter.instruction("sub rcx, r9");                                         // room left in the shared concat buffer
    emitter.instruction("jb __rt_implode_int_fallback_x86");                    // an over-full scratch join uses the growable body
    emitter.instruction("cmp rax, rcx");                                        // does the worst-case output fit the remaining capacity?
    emitter.instruction("ja __rt_implode_int_fallback_x86");                    // larger joins use the growable body
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute the current concat-buffer destination pointer for the integer implode output
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the implode result start pointer so the final string result can reference the copied bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the current concat-buffer destination cursor across glue emission and integer string copies
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the indexed-array logical length once before entering the integer implode loop
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the indexed-array logical length for the loop termination check
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the indexed-array loop cursor to the first integer element

    emitter.label("__rt_implode_int_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before deciding whether integer implode is complete
    emitter.instruction("cmp r11, QWORD PTR [rbp - 48]");                       // compare the current indexed-array loop cursor against the saved logical length
    emitter.instruction("jae __rt_implode_int_done");                           // stop once every indexed-array integer element has been copied into the concat buffer
    emitter.instruction("test r11, r11");                                       // check whether the current integer element is the first one in the indexed array
    emitter.instruction("jz __rt_implode_int_elem");                            // skip glue emission before converting the first integer element
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the glue string pointer before copying the separator bytes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the glue string length before copying the separator bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the separator bytes
    emitter.instruction("cmp r9, 1");                                           // is the separator exactly one byte?
    emitter.instruction("jne __rt_implode_int_glue");                           // other lengths keep the general copy loop
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load the single separator byte
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store it in the concat buffer
    emitter.instruction("add r10, 1");                                          // advance the destination cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the advanced destination cursor
    emitter.instruction("jmp __rt_implode_int_elem");                           // continue with the integer element

    emitter.label("__rt_implode_int_glue");
    emitter.instruction("test r9, r9");                                         // check whether every glue byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_int_glue_done");                       // continue with integer conversion once the glue string has been fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the glue string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one separator byte into the concat buffer before advancing the destination pointer
    emitter.instruction("add r8, 1");                                           // advance the glue string source pointer after copying one separator byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one separator byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining glue byte count after copying one separator byte
    emitter.instruction("jmp __rt_implode_int_glue");                           // continue copying separator bytes until the glue string is exhausted

    emitter.label("__rt_implode_int_glue_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the separator bytes

    emitter.label("__rt_implode_int_elem");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before locating the next integer element slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the indexed-array pointer before addressing the current integer slot
    emitter.instruction("mov rax, QWORD PTR [r10 + r11 * 8 + 24]");             // load the current indexed-array integer payload into the integer-to-string helper input register
    // -- small non-negative values format without the scratch-backed itoa helper --
    emitter.instruction("cmp rax, 100");                                        // do the common small values fit the direct formatter?
    emitter.instruction("jae __rt_implode_int_small_slow_x86");                 // larger and negative values keep the generic itoa path
    emitter.instruction("cmp rax, 10");                                         // is the value a single decimal digit?
    emitter.instruction("jae __rt_implode_int_small_two_x86");                  // two-digit values divide once instead of looping
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // reload the destination cursor for the direct write
    emitter.instruction("add al, 48");                                          // convert the single digit to ASCII
    emitter.instruction("mov BYTE PTR [r8], al");                               // store the digit
    emitter.instruction("add r8, 1");                                           // advance the destination cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve the advanced destination cursor
    emitter.instruction("jmp __rt_implode_int_next");                           // advance to the next array element

    emitter.label("__rt_implode_int_small_two_x86");
    emitter.instruction("mov r8d, eax");                                        // preserve the original value for the remainder
    emitter.instruction("imul eax, eax, 205");                                  // scale the value for the reciprocal division
    emitter.instruction("shr eax, 11");                                         // eax = floor(v / 10) for v in 0..99
    emitter.instruction("lea ecx, [rax + rax * 4]");                            // ecx = quotients times five
    emitter.instruction("add ecx, ecx");                                        // ecx = quotients times ten
    emitter.instruction("sub r8d, ecx");                                        // r8d = ones digit
    emitter.instruction("add al, 48");                                          // convert the tens digit to ASCII
    emitter.instruction("add r8b, 48");                                         // convert the ones digit to ASCII
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the destination cursor for the direct write
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the tens digit
    emitter.instruction("mov BYTE PTR [r9 + 1], r8b");                          // store the ones digit
    emitter.instruction("add r9, 2");                                           // advance the destination cursor past both digits
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the advanced destination cursor
    emitter.instruction("jmp __rt_implode_int_next");                           // advance to the next array element

    emitter.label("__rt_implode_int_small_slow_x86");
    emitter.instruction("mov rdi, rax");                                        // pass the integer value to the private formatting helper
    emitter.instruction("lea rsi, [rbp - 96]");                                 // point at the private 21-byte scratch window
    emitter.instruction("call __rt_itoa_into");                                 // format without touching the shared concat buffer
    emitter.instruction("mov r8, rax");                                         // preserve the returned decimal string pointer before copying bytes
    emitter.instruction("mov r9, rdx");                                         // preserve the returned decimal string length before copying bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the converted decimal bytes

    emitter.label("__rt_implode_int_copy");
    emitter.instruction("test r9, r9");                                         // check whether every converted decimal byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_int_slow_done");                       // save the advanced cursor once the converted decimal bytes are copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the converted decimal string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one byte from the converted decimal string into the concat buffer
    emitter.instruction("add r8, 1");                                           // advance the converted decimal string source pointer after copying one byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining converted decimal byte count after copying one byte
    emitter.instruction("jmp __rt_implode_int_copy");                           // continue copying bytes from the converted decimal string until it is exhausted

    emitter.label("__rt_implode_int_slow_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the advanced destination cursor after the slow decimal copy
    emitter.label("__rt_implode_int_next");
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // advance the indexed-array loop cursor to the next integer element
    emitter.instruction("jmp __rt_implode_int_loop");                           // continue joining converted integer elements into the concat buffer

    emitter.label("__rt_implode_int_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final concat-buffer destination cursor to compute the joined string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the implode result start pointer before computing the joined string length
    emitter.instruction("mov rdx, r10");                                        // copy the final concat-buffer destination cursor before subtracting the result start pointer
    emitter.instruction("sub rdx, rax");                                        // compute the joined string length as dest_end - dest_start
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // reload the current concat-buffer write offset after the integer-to-string helper scratch allocations
    emitter.instruction("add r9, rdx");                                         // advance the concat-buffer write offset by the joined string length that this integer implode call produced
    emitter.instruction("mov QWORD PTR [r8], r9");                              // persist the updated concat-buffer write offset after writing the integer implode output bytes
    emitter.instruction("add rsp, 112");                                        // release the integer-implode frame before returning the joined string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the joined string
    emitter.instruction("ret");                                                 // return the joined string in the standard x86_64 string result registers

    emitter.label("__rt_implode_int_fallback_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the glue pointer for the shared join
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the glue length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the packed array pointer
    emitter.instruction("xor ecx, ecx");                                        // force the raw-int element layout
    emitter.instruction("add rsp, 112");                                        // release the fast-path frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_implode_typed");                              // join through the bounded, growable body
}
