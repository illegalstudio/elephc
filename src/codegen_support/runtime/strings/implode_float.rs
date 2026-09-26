//! Purpose:
//! Emits the `__rt_implode_float` runtime helper assembly for `implode()` over float arrays.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - A homogeneous float array stores raw 8-byte doubles after the 24-byte array header, so
//!   neither the string-slot walk of `__rt_implode` nor the integer walk of `__rt_implode_int`
//!   can read it (#640).
//! - `__rt_ftoa` appends its text at `_concat_off` and advances it. This helper keeps
//!   `_concat_off` equal to the output cursor: glue bytes are written at `_concat_off` and the
//!   offset is advanced past them, then `__rt_ftoa` appends the element in place. Nothing is
//!   copied, so the conversion can never overwrite glue already written.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits runtime helper for PHP `implode()` with float array elements.
/// ABI: x1/x2=glue_ptr/glue_len, x3=array_ptr → x1=result_ptr, x2=result_len.
/// The result lives in the shared concat buffer, and `_concat_off` ends just past it.
pub fn emit_implode_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_float_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode_float ---");
    emitter.label_global("__rt_implode_float");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate spill slots for glue, array, result start, count and index
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save array pointer

    // -- the result starts at the current concat_buf write position --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute the result start pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer
    emitter.instruction("ldr x10, [x3]");                                       // load array element count
    emitter.instruction("str x10, [sp, #32]");                                  // save element count
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize element index = 0

    // -- main loop: join elements with glue --
    emitter.label("__rt_implode_float_loop");
    emitter.instruction("ldr x11, [sp, #40]");                                  // load current element index
    emitter.instruction("ldr x10, [sp, #32]");                                  // load element count
    emitter.instruction("cmp x11, x10");                                        // check if all elements processed
    emitter.instruction("b.ge __rt_implode_float_done");                        // if done, finalize result
    emitter.instruction("cbz x11, __rt_implode_float_elem");                    // skip glue before first element

    // -- append glue at concat_off and advance the offset past it --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute the glue destination pointer
    emitter.instruction("add x8, x8, x2");                                      // the offset moves past the glue bytes
    emitter.instruction("str x8, [x6]");                                        // publish the advanced write offset
    emitter.label("__rt_implode_float_glue");
    emitter.instruction("cbz x2, __rt_implode_float_elem");                     // if no glue bytes remain, append the element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load glue byte, advance glue ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement glue byte counter
    emitter.instruction("b __rt_implode_float_glue");                           // continue copying glue

    // -- append the current float element through ftoa --
    emitter.label("__rt_implode_float_elem");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload current element index
    emitter.instruction("add x3, x3, #24");                                     // skip 24-byte array header to reach data
    emitter.instruction("ldr d0, [x3, x11, lsl #3]");                           // load the float element at index (8 bytes each)
    emitter.instruction("bl __rt_ftoa");                                        // append its PHP text at concat_off and advance the offset
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload element index
    emitter.instruction("add x11, x11, #1");                                    // increment element index
    emitter.instruction("str x11, [sp, #40]");                                  // save updated index
    emitter.instruction("b __rt_implode_float_loop");                           // process next element

    // -- finalize: the result runs from its start to the current concat_off --
    emitter.label("__rt_implode_float_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load the final write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute the result end pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = end - start

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_implode_float` runtime helper for Linux x86_64.
/// ABI: rdi/rsi=glue_ptr/glue_len, rdx=array_ptr → rax=result_ptr, rdx=result_len.
/// Same scheme as the ARM64 body: glue and each `__rt_ftoa` text are appended at `_concat_off`.
fn emit_implode_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode_float ---");
    emitter.label_global("__rt_implode_float");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the spill slots
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for glue, array, result start, count and index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the glue string pointer across ftoa calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the glue string length across ftoa calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the indexed-array pointer across ftoa calls
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the result start pointer
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the indexed-array logical length
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the length for the loop termination check
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the loop cursor to the first element

    emitter.label("__rt_implode_float_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the loop cursor
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // compare it against the saved length
    emitter.instruction("jae __rt_implode_float_done");                         // stop once every element has been appended
    emitter.instruction("test r11, r11");                                       // is this the first element?
    emitter.instruction("jz __rt_implode_float_elem");                          // skip glue before the first element
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the glue string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the glue string length
    crate::codegen_support::abi::emit_symbol_address(emitter, "rcx", "_concat_off");
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // load the current concat-buffer write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + rax]");                                // compute the glue destination pointer
    emitter.instruction("add rax, r9");                                         // the offset moves past the glue bytes
    emitter.instruction("mov QWORD PTR [rcx], rax");                            // publish the advanced write offset

    emitter.label("__rt_implode_float_glue");
    emitter.instruction("test r9, r9");                                         // have all glue bytes been copied?
    emitter.instruction("jz __rt_implode_float_elem");                          // append the element once the glue is copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one glue byte
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store it into the concat buffer
    emitter.instruction("add r8, 1");                                           // advance the glue source pointer
    emitter.instruction("add r10, 1");                                          // advance the destination pointer
    emitter.instruction("sub r9, 1");                                           // decrement the remaining glue byte count
    emitter.instruction("jmp __rt_implode_float_glue");                         // continue copying glue bytes

    emitter.label("__rt_implode_float_elem");
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the loop cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the indexed-array pointer
    emitter.instruction("movsd xmm0, QWORD PTR [r10 + r11 * 8 + 24]");          // load the current float element into the ftoa input register
    emitter.instruction("call __rt_ftoa");                                      // append its PHP text at concat_off and advance the offset
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance the loop cursor to the next element
    emitter.instruction("jmp __rt_implode_float_loop");                         // continue joining elements

    emitter.label("__rt_implode_float_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the result start pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the final concat-buffer write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea rdx, [r10 + r9]");                                 // compute the result end pointer
    emitter.instruction("sub rdx, rax");                                        // result length = end - start
    emitter.instruction("add rsp, 48");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the joined string in rax/rdx
}
