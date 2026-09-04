//! Purpose:
//! Emits the `__rt_implode_float` runtime helper: the `implode()`/`join()` element renderer for
//! an indexed array whose slots hold raw 8-byte IEEE-754 doubles (runtime `value_type` tag 2).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - Selected by `implode_runtime_label` for a statically `array<float>` / `array<string, float>`
//!   operand, and tail-branched to by `__rt_implode_dyn` on value_type tag 2.
//!
//! Key details:
//! - Shares the renderer ABI of `__rt_implode` / `__rt_implode_int` / `__rt_implode_bool` so that
//!   `__rt_implode_dyn` can reach it with a plain branch: AArch64 `x1`/`x2` = glue ptr/len,
//!   `x3` = array ptr → `x1`/`x2` = result ptr/len; x86_64 `rdi`/`rsi` = glue ptr/len,
//!   `rdx` = array ptr → `rax`/`rdx` = result ptr/len.
//! - Element bytes come from `__rt_ftoa`, PHP's `precision = 14` / `zend_gcvt` layout — the same
//!   formatter `echo $float` uses. Measured with `php -n` (8.5.6):
//!   `implode(",", [1.5, 2.0, 1e20, 0.1+0.2, -0.0, INF])` is `1.5,2,1.0E+20,0.3,-0,INF`.
//! - CURSOR: `__rt_ftoa` formats into `_concat_buf` at `_concat_off` and advances the offset by the
//!   bytes it actually wrote — unlike `__rt_itoa`, which always reserves a fixed 21-byte scratch.
//!   Leaving `_concat_off` parked at the implode result START therefore made the SECOND element's
//!   conversion overwrite the glue bytes already copied. The LIVE destination cursor is published
//!   as `_concat_off` before every conversion, exactly as `__rt_implode`'s boxed-Mixed arm does,
//!   and the ABSOLUTE end offset is stamped on completion rather than added to whatever
//!   `__rt_ftoa` left behind.
//! - Publishing the live cursor makes `__rt_ftoa` write its digits directly at the destination, so
//!   the copy loop below is a self-copy. It is kept because the loop is what advances the cursor,
//!   and because nothing in `__rt_ftoa`'s contract promises the returned pointer equals the cursor.
//! - The helper owns no allocation: `__rt_ftoa` returns concat-buffer storage, never a heap block,
//!   so there is nothing to release per element (contrast `__rt_implode`'s `#601` mixed-cast slot).

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_implode_float` runtime helper for joining raw `f64` array elements.
///
/// ABI (AArch64): x1/x2 = glue_ptr/glue_len, x3 = array_ptr → x1 = result_ptr, x2 = result_len.
/// ABI (x86_64): rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
/// Writes the joined bytes into the shared concat buffer and stamps `_concat_off` one past them.
pub fn emit_implode_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_implode_float_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: implode_float (precision=14 float elements) ---");
    emitter.label_global("__rt_implode_float");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate the float-implode spill slots and saved-register area
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save array pointer

    // -- get concat_buf write position --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x6", "_concat_off");
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x7", "_concat_buf");
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer
    emitter.instruction("str x6, [sp, #32]");                                   // save offset variable address
    emitter.instruction("str x9, [sp, #40]");                                   // save current dest pointer

    // -- load array length and initialize index --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x10, [x3]");                                       // load array element count
    emitter.instruction("str x10, [sp, #48]");                                  // save element count
    emitter.instruction("str xzr, [sp, #56]");                                  // initialize element index = 0

    // -- main loop: join elements with glue --
    emitter.label("__rt_implode_float_loop");
    emitter.instruction("ldr x11, [sp, #56]");                                  // load current element index
    emitter.instruction("ldr x10, [sp, #48]");                                  // load element count
    emitter.instruction("cmp x11, x10");                                        // check if all elements processed
    emitter.instruction("b.ge __rt_implode_float_done");                        // if done, finalize result
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the live destination cursor for this element

    // -- insert glue before element (skip for first element) --
    emitter.instruction("cbz x11, __rt_implode_float_elem");                    // skip glue before first element
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    emitter.instruction("mov x12, x2");                                         // copy glue length as counter
    emitter.label("__rt_implode_float_glue");
    emitter.instruction("cbz x12, __rt_implode_float_elem");                    // if no glue bytes remain, convert the element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load glue byte, advance glue ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement glue byte counter
    emitter.instruction("b __rt_implode_float_glue");                           // continue copying glue

    // -- convert the current double element through PHP's precision=14 formatter --
    emitter.label("__rt_implode_float_elem");
    emitter.instruction("str x9, [sp, #40]");                                   // save updated dest pointer
    // Publish the LIVE destination cursor as `_concat_off`. `__rt_ftoa` formats into `_concat_buf`
    // at that offset and advances it by the bytes it wrote; parking the offset at the implode
    // result START made the next conversion overwrite the glue already copied.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_concat_buf");
    emitter.instruction("sub x14, x9, x13");                                    // absolute offset of the live implode destination cursor
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload the concat offset variable address
    emitter.instruction("str x14, [x13]");                                      // reserve everything written so far against the conversion scratch
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload current element index
    emitter.instruction("add x3, x3, #24");                                     // skip 24-byte array header to reach the payload region
    emitter.instruction("ldr d0, [x3, x11, lsl #3]");                           // load the raw f64 element at index (8 bytes each)
    emitter.instruction("bl __rt_ftoa");                                        // convert the double to PHP's precision=14 spelling → x1=ptr, x2=len

    // -- copy the formatted bytes to output --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload dest pointer
    emitter.instruction("mov x12, x2");                                         // copy string length as counter
    emitter.label("__rt_implode_float_copy");
    emitter.instruction("cbz x12, __rt_implode_float_next");                    // if no bytes remain, move to next element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load string byte, advance src ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement byte counter
    emitter.instruction("b __rt_implode_float_copy");                           // continue copying string

    // -- advance to next element --
    emitter.label("__rt_implode_float_next");
    emitter.instruction("str x9, [sp, #40]");                                   // save updated dest pointer
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload element index
    emitter.instruction("add x11, x11, #1");                                    // increment element index
    emitter.instruction("str x11, [sp, #56]");                                  // save updated index
    emitter.instruction("b __rt_implode_float_loop");                           // process next element

    // -- finalize: compute result length and stamp the absolute concat_off --
    emitter.label("__rt_implode_float_done");
    emitter.instruction("ldr x9, [sp, #40]");                                   // load final dest pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x6, [sp, #32]");                                   // load offset variable address
    // Stamp the ABSOLUTE end offset: the last conversion left `_concat_off` at its own scratch end,
    // which is inside the region this call has just filled with the joined result.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_concat_buf");
    emitter.instruction("sub x14, x9, x13");                                    // absolute offset one past the joined result
    emitter.instruction("str x14, [x6]");                                       // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_implode_float` for Linux x86_64.
///
/// ABI: rdi/rsi = glue_ptr/glue_len, rdx = array_ptr → rax = result_ptr, rdx = result_len.
/// Stack frame: 64 bytes of spill slots for glue, array, destination cursor, length, and index.
fn emit_implode_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode_float (precision=14 float elements) ---");
    emitter.label_global("__rt_implode_float");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving float-implode spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for glue, array, and concat-buffer bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for glue, array, concat destination, array length, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the glue string pointer across float conversion and copy helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the glue string length across float conversion and copy helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the indexed-array pointer across float conversion and copy helper calls
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer write offset before materializing the implode output start pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r10, [r10 + r9]");                                 // compute the current concat-buffer destination pointer for the float implode output
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the implode result start pointer so the final string result can reference the copied bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the current concat-buffer destination cursor across glue emission and float string copies
    emitter.instruction("mov r11, QWORD PTR [rdx]");                            // load the indexed-array logical length once before entering the float implode loop
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the indexed-array logical length for the loop termination check
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the indexed-array loop cursor to the first float element

    emitter.label("__rt_implode_float_loop");
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before deciding whether float implode is complete
    emitter.instruction("cmp r11, QWORD PTR [rbp - 48]");                       // compare the current indexed-array loop cursor against the saved logical length
    emitter.instruction("jae __rt_implode_float_done");                         // stop once every indexed-array float element has been copied into the concat buffer
    emitter.instruction("test r11, r11");                                       // check whether the current float element is the first one in the indexed array
    emitter.instruction("jz __rt_implode_float_elem");                          // skip glue emission before converting the first float element
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the glue string pointer before copying the separator bytes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the glue string length before copying the separator bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the separator bytes

    emitter.label("__rt_implode_float_glue");
    emitter.instruction("test r9, r9");                                         // check whether every glue byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_float_glue_done");                     // continue with float conversion once the glue string has been fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the glue string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one separator byte into the concat buffer before advancing the destination pointer
    emitter.instruction("add r8, 1");                                           // advance the glue string source pointer after copying one separator byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one separator byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining glue byte count after copying one separator byte
    emitter.instruction("jmp __rt_implode_float_glue");                         // continue copying separator bytes until the glue string is exhausted

    emitter.label("__rt_implode_float_glue_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the separator bytes

    emitter.label("__rt_implode_float_elem");
    // Publish the LIVE destination cursor as `_concat_off`. `__rt_ftoa` formats into `_concat_buf`
    // at that offset and advances it by the bytes it wrote; parking the offset at the implode
    // result START made the next conversion overwrite the glue already copied.
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the live implode destination cursor
    emitter.instruction("sub r9, r8");                                          // absolute offset of the live implode destination cursor
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov QWORD PTR [r8], r9");                              // reserve everything written so far against the conversion scratch
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current indexed-array loop cursor before locating the next float element slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the indexed-array pointer before addressing the current float slot
    emitter.instruction("movsd xmm0, QWORD PTR [r10 + r11 * 8 + 24]");          // load the current indexed-array f64 payload into the float-to-string helper input register
    emitter.instruction("call __rt_ftoa");                                      // convert the current indexed-array float element into a concat-buffer-backed decimal string
    emitter.instruction("mov r8, rax");                                         // preserve the decimal string pointer returned by the float-to-string helper before copying bytes
    emitter.instruction("mov r9, rdx");                                         // preserve the decimal string length returned by the float-to-string helper before copying bytes
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current concat-buffer destination cursor before copying the converted decimal bytes

    emitter.label("__rt_implode_float_copy");
    emitter.instruction("test r9, r9");                                         // check whether every converted decimal byte has already been copied into the concat buffer
    emitter.instruction("jz __rt_implode_float_next");                          // advance to the next indexed-array float once the current decimal string is fully copied
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the converted decimal string before advancing the source pointer
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store one byte from the converted decimal string into the concat buffer
    emitter.instruction("add r8, 1");                                           // advance the converted decimal string source pointer after copying one byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer destination pointer after storing one byte
    emitter.instruction("sub r9, 1");                                           // decrement the remaining converted decimal byte count after copying one byte
    emitter.instruction("jmp __rt_implode_float_copy");                         // continue copying bytes from the converted decimal string until it is exhausted

    emitter.label("__rt_implode_float_next");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the concat-buffer destination cursor after copying the current converted decimal string
    emitter.instruction("add QWORD PTR [rbp - 56], 1");                         // advance the indexed-array loop cursor to the next float element
    emitter.instruction("jmp __rt_implode_float_loop");                         // continue joining converted float elements into the concat buffer

    emitter.label("__rt_implode_float_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final concat-buffer destination cursor to compute the joined string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the implode result start pointer before computing the joined string length
    emitter.instruction("mov rdx, r10");                                        // copy the final concat-buffer destination cursor before subtracting the result start pointer
    emitter.instruction("sub rdx, rax");                                        // compute the joined string length as dest_end - dest_start
    // Stamp the ABSOLUTE end offset: the last conversion left `_concat_off` at its own scratch end,
    // which is inside the region this call has just filled with the joined result.
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov r9, r10");                                         // copy the final destination cursor before converting it to an absolute offset
    emitter.instruction("sub r9, r8");                                          // absolute offset one past the joined result
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov QWORD PTR [r8], r9");                              // persist the updated concat-buffer write offset after writing the float implode output bytes
    emitter.instruction("add rsp, 64");                                         // release the float-implode spill slots before returning the joined string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the joined string
    emitter.instruction("ret");                                                 // return the joined string in the standard x86_64 string result registers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies the ARM64 float renderer publishes the live cursor before every `__rt_ftoa`
    /// conversion and stamps the absolute end offset, with a balanced 80-byte frame.
    #[test]
    fn test_implode_float_arm64_publishes_live_cursor() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_implode_float(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_implode_float:\n"));
        assert!(asm.contains("ldr d0, [x3, x11, lsl #3]"));
        assert!(asm.contains("bl __rt_ftoa"));
        // the live cursor is published (sub x14, x9, x13 → str x14, [x13]) before the conversion
        assert!(asm.contains("sub x14, x9, x13"));
        assert!(asm.contains("str x14, [x13]"));
        // and the absolute end offset is stamped on completion
        assert!(asm.contains("str x14, [x6]"));
        assert_eq!(asm.matches("sub sp, sp, #80").count(), 1);
        assert_eq!(asm.matches("add sp, sp, #80").count(), 1);
    }

    /// Verifies the x86_64 float renderer loads the payload into `xmm0`, publishes the live
    /// cursor before `__rt_ftoa`, and balances its 64-byte frame.
    #[test]
    fn test_implode_float_x86_64_publishes_live_cursor() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_implode_float(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_implode_float:\n"));
        assert!(asm.contains("movsd xmm0, QWORD PTR [r10 + r11 * 8 + 24]"));
        assert!(asm.contains("call __rt_ftoa"));
        assert!(asm.contains("mov QWORD PTR [r8], r9"));
        assert_eq!(asm.matches("sub rsp, 64").count(), 1);
        assert_eq!(asm.matches("add rsp, 64").count(), 1);
    }
}
