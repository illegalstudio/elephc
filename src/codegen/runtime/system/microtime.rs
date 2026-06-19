//! Purpose:
//! Emits the `__rt_microtime` runtime helper assembly for microtime.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - System helpers must preserve PHP-visible behavior while crossing libc, syscall, JSON, regex, and date formatter boundaries.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_microtime: get current time as float seconds via gettimeofday syscall.
/// Output: d0 = seconds.microseconds as float
pub(crate) fn emit_microtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_microtime_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: microtime ---");
    emitter.label_global("__rt_microtime");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes (16 for timeval + 16 for frame + padding)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    // -- call gettimeofday syscall --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timeval struct on stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.syscall(116);

    // -- extract tv_sec and tv_usec --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = tv_sec (seconds)
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = tv_usec (microseconds)

    // -- convert to float: d0 = tv_sec + tv_usec / 1000000.0 --
    emitter.instruction("scvtf d0, x0");                                        // d0 = (double)tv_sec
    emitter.instruction("scvtf d1, x1");                                        // d1 = (double)tv_usec
    emitter.instruction("movz x9, #0x4240");                                    // x9 = lower 16 bits of 1000000 (0x0F4240)
    emitter.instruction("movk x9, #0x000F, lsl #16");                           // x9 = 1000000 (microseconds per second)
    emitter.instruction("scvtf d2, x9");                                        // d2 = 1000000.0
    emitter.instruction("fdiv d1, d1, d2");                                     // d1 = tv_usec / 1000000.0
    emitter.instruction("fadd d0, d0, d1");                                     // d0 = tv_sec + tv_usec/1000000.0

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_microtime` using libc `gettimeofday`.
/// Uses the SysV AMD64 ABI: arguments in rdi, rsi; floating-point result in xmm0.
/// Output: xmm0 = tv_sec + tv_usec / 1000000.0 as double.
fn emit_microtime_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: microtime ---");
    emitter.label_global("__rt_microtime");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating the temporary timeval storage for libc gettimeofday()
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary timeval storage used by libc gettimeofday()
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack storage for one timeval struct plus scratch padding before the libc call
    emitter.instruction("lea rdi, [rsp]");                                      // pass the temporary timeval storage as the first SysV integer argument to libc gettimeofday()
    emitter.instruction("xor esi, esi");                                        // pass NULL as the timezone pointer because elephc only needs the current Unix timestamp
    emitter.bl_c("gettimeofday");                                               // fill the temporary timeval with the current wall-clock time through libc
    emitter.instruction("cvtsi2sd xmm0, QWORD PTR [rsp]");                      // convert tv_sec from the temporary timeval into the base double-precision second count
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rsp + 8]");                  // convert tv_usec from the temporary timeval into a double-precision microsecond count
    emitter.instruction("mov r10, 1000000");                                    // materialize the number of microseconds per second before converting it into a floating divisor
    emitter.instruction("cvtsi2sd xmm2, r10");                                  // convert the microseconds-per-second divisor into double precision for the fractional-second normalization
    emitter.instruction("divsd xmm1, xmm2");                                    // normalize the microsecond count into the fractional-second component of the final result
    emitter.instruction("addsd xmm0, xmm1");                                    // combine the whole-second and fractional-second components into the final double-precision timestamp
    emitter.instruction("leave");                                               // release the temporary timeval storage and restore the caller frame pointer in one step
    emitter.instruction("ret");                                                 // return the floating-point Unix timestamp to generated code
}

/// __rt_microtime_build_into: write the `microtime()` string form into a caller buffer.
/// Input:  x0 = destination buffer pointer (at least 32 bytes).
/// Output: x0 = number of bytes written.
/// Performs one gettimeofday, then writes "0." + 8 zero-padded digits of (usec*100) + " " +
/// decimal seconds into the buffer (seconds extracted least-significant first then reversed)
/// and returns the total length. Shared by `__rt_microtime_str` (heap-owned via str_persist) and
/// `__rt_microtime_mixed` (box tag 1, persisted by mixed_from_value). Never touches _concat_buf.
pub(crate) fn emit_microtime_build_into(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_microtime_build_into_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: microtime_build_into ---");
    emitter.label_global("__rt_microtime_build_into");

    // -- set up stack frame: [sp,#0..16] timeval, [sp,#16..24] buf save, [sp,#32..48] frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate the timeval, buffer-pointer save, and saved frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set the new frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the destination buffer pointer across the syscall

    // -- call gettimeofday --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to the timeval struct on the stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.syscall(116);

    // -- reload the buffer and write the "0." prefix --
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = destination buffer (reloaded after the syscall)
    emitter.instruction("mov w10, #0x30");                                      // ASCII '0'
    emitter.instruction("strb w10, [x9, #0]");                                  // write the leading '0'
    emitter.instruction("mov w10, #0x2e");                                      // ASCII '.'
    emitter.instruction("strb w10, [x9, #1]");                                  // write the decimal point

    // -- write 8 zero-padded digits of usec*100, most-significant first --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = tv_usec (microseconds)
    emitter.instruction("mov x11, #100");                                       // scale microseconds by 100 into 8 fractional digits
    emitter.instruction("mul x1, x1, x11");                                     // x1 = usec * 100 (0 ..= 99999900, fits 8 digits)
    emitter.instruction("mov x11, #10");                                        // x11 = decimal divisor for digit extraction
    emitter.instruction("add x12, x9, #9");                                     // x12 = &buf[9], the most-significant write position (descends)
    emitter.instruction("mov x13, #8");                                         // eight fractional digits to emit
    emitter.label("__rt_microtime_build_into_usec");
    emitter.instruction("udiv x14, x1, x11");                                   // x14 = usec_scaled / 10
    emitter.instruction("msub x15, x14, x11, x1");                              // x15 = usec_scaled % 10 (current least-significant digit)
    emitter.instruction("add w15, w15, #0x30");                                 // convert the digit to ASCII
    emitter.instruction("strb w15, [x12]");                                     // write the digit at the current position
    emitter.instruction("sub x12, x12, #1");                                    // advance to the next more-significant position
    emitter.instruction("mov x1, x14");                                         // usec_scaled = usec_scaled / 10 for the next digit
    emitter.instruction("subs x13, x13, #1");                                   // one fewer digit remaining
    emitter.instruction("b.ne __rt_microtime_build_into_usec");                 // repeat until all eight digits are written

    // -- write the separating space --
    emitter.instruction("mov w10, #0x20");                                      // ASCII space
    emitter.instruction("strb w10, [x9, #10]");                                 // write the space between the fractional and second parts

    // -- write decimal seconds, least-significant first, then reverse in place --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = tv_sec (seconds)
    emitter.instruction("add x12, x9, #11");                                    // x12 = &buf[11], the seconds write position (ascends)
    emitter.instruction("cbz x1, __rt_microtime_build_into_sec_zero");          // a zero second needs the single '0' digit
    emitter.label("__rt_microtime_build_into_sec_loop");
    emitter.instruction("udiv x14, x1, x11");                                   // x14 = seconds / 10
    emitter.instruction("msub x15, x14, x11, x1");                              // x15 = seconds % 10 (current least-significant digit)
    emitter.instruction("add w15, w15, #0x30");                                 // convert the digit to ASCII
    emitter.instruction("strb w15, [x12], #1");                                 // write the digit and advance the write position
    emitter.instruction("mov x1, x14");                                         // seconds = seconds / 10 for the next digit
    emitter.instruction("cbnz x1, __rt_microtime_build_into_sec_loop");         // repeat until all seconds digits are written
    emitter.instruction("b __rt_microtime_build_into_rev");                     // proceed to reverse the least-significant-first digits
    emitter.label("__rt_microtime_build_into_sec_zero");
    emitter.instruction("mov w15, #0x30");                                      // ASCII '0'
    emitter.instruction("strb w15, [x12], #1");                                 // write the single zero second digit
    emitter.label("__rt_microtime_build_into_rev");
    emitter.instruction("add x13, x9, #11");                                    // x13 = left edge of the seconds slice
    emitter.instruction("sub x14, x12, #1");                                    // x14 = right edge of the seconds slice
    emitter.label("__rt_microtime_build_into_rev_loop");
    emitter.instruction("cmp x13, x14");                                        // have the swap pointers crossed?
    emitter.instruction("b.ge __rt_microtime_build_into_done");                 // yes -> the seconds digits are now most-significant first
    emitter.instruction("ldrb w15, [x13]");                                     // load the left byte
    emitter.instruction("ldrb w10, [x14]");                                     // load the right byte
    emitter.instruction("strb w10, [x13]");                                     // write the right byte to the left position
    emitter.instruction("strb w15, [x14]");                                     // write the left byte to the right position
    emitter.instruction("add x13, x13, #1");                                    // advance the left pointer
    emitter.instruction("sub x14, x14, #1");                                    // retreat the right pointer
    emitter.instruction("b __rt_microtime_build_into_rev_loop");                // continue swapping until the pointers meet
    emitter.label("__rt_microtime_build_into_done");
    emitter.instruction("sub x0, x12, x9");                                     // x0 = bytes written (write position minus buffer start)

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the byte count in x0
}

/// x86_64 Linux variant of `__rt_microtime_build_into`.
/// Input:  rdi = destination buffer pointer (at least 32 bytes).
/// Output: rax = number of bytes written. Uses libc gettimeofday; the buffer pointer is spilled
/// across the libc call because the SysV ABI clobbers the caller-saved argument registers.
fn emit_microtime_build_into_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: microtime_build_into ---");
    emitter.label_global("__rt_microtime_build_into");

    // -- set up stack frame: [rsp+0..16] timeval, [rsp+16..24] buf save, [rsp+24..48] scratch --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the libc gettimeofday call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the timeval and buffer bookkeeping
    emitter.instruction("sub rsp, 48");                                         // reserve aligned storage for the timeval, saved buffer, and scratch
    emitter.instruction("mov QWORD PTR [rsp + 16], rdi");                       // save the destination buffer pointer across the libc call

    // -- call gettimeofday --
    emitter.instruction("lea rdi, [rsp]");                                      // rdi = pointer to the timeval storage
    emitter.instruction("xor esi, esi");                                        // rsi = NULL (timezone not needed)
    emitter.bl_c("gettimeofday");                                          // fill the timeval with the current wall-clock time

    // -- reload the buffer and write the "0." prefix --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // rdi = destination buffer (reloaded after the libc call)
    emitter.instruction("mov BYTE PTR [rdi], 0x30");                            // write the leading '0'
    emitter.instruction("mov BYTE PTR [rdi + 1], 0x2e");                        // write the decimal point

    // -- write 8 zero-padded digits of usec*100, most-significant first --
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // rax = tv_usec (microseconds)
    emitter.instruction("imul rax, rax, 100");                                  // rax = usec * 100 (0 ..= 99999900, fits 8 digits)
    emitter.instruction("mov rcx, 10");                                         // rcx = decimal divisor for digit extraction
    emitter.instruction("lea rsi, [rdi + 9]");                                  // rsi = &buf[9], the most-significant write position (descends)
    emitter.instruction("mov r8, 8");                                           // eight fractional digits to emit
    emitter.label("__rt_microtime_build_into_usec");
    emitter.instruction("xor rdx, rdx");                                        // clear the dividend high word before the unsigned divide
    emitter.instruction("div rcx");                                             // rax = usec_scaled / 10, rdx = usec_scaled % 10
    emitter.instruction("add dl, 0x30");                                        // convert the remainder digit to ASCII
    emitter.instruction("mov BYTE PTR [rsi], dl");                              // write the digit at the current position
    emitter.instruction("dec rsi");                                             // advance to the next more-significant position
    emitter.instruction("dec r8");                                              // one fewer digit remaining
    emitter.instruction("jnz __rt_microtime_build_into_usec");                  // repeat until all eight digits are written

    // -- write the separating space --
    emitter.instruction("mov BYTE PTR [rdi + 10], 0x20");                       // write the space between the fractional and second parts

    // -- write decimal seconds, least-significant first, then reverse in place --
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // rax = tv_sec (seconds)
    emitter.instruction("lea rsi, [rdi + 11]");                                 // rsi = &buf[11], the seconds write position (ascends)
    emitter.instruction("test rax, rax");                                       // is the second count zero?
    emitter.instruction("jz __rt_microtime_build_into_sec_zero");               // a zero second needs the single '0' digit
    emitter.label("__rt_microtime_build_into_sec_loop");
    emitter.instruction("xor rdx, rdx");                                        // clear the dividend high word before the unsigned divide
    emitter.instruction("div rcx");                                             // rax = seconds / 10, rdx = seconds % 10
    emitter.instruction("add dl, 0x30");                                        // convert the remainder digit to ASCII
    emitter.instruction("mov BYTE PTR [rsi], dl");                              // write the digit at the current position
    emitter.instruction("inc rsi");                                             // advance to the next write position
    emitter.instruction("test rax, rax");                                       // are there more seconds digits?
    emitter.instruction("jnz __rt_microtime_build_into_sec_loop");              // repeat until all seconds digits are written
    emitter.instruction("jmp __rt_microtime_build_into_rev");                   // proceed to reverse the least-significant-first digits
    emitter.label("__rt_microtime_build_into_sec_zero");
    emitter.instruction("mov BYTE PTR [rsi], 0x30");                            // write the single zero second digit
    emitter.instruction("inc rsi");                                             // advance past the zero digit
    emitter.label("__rt_microtime_build_into_rev");
    emitter.instruction("lea r9, [rdi + 11]");                                  // r9 = left edge of the seconds slice
    emitter.instruction("lea r10, [rsi - 1]");                                  // r10 = right edge of the seconds slice
    emitter.label("__rt_microtime_build_into_rev_loop");
    emitter.instruction("cmp r9, r10");                                         // have the swap pointers crossed?
    emitter.instruction("jge __rt_microtime_build_into_done");                  // yes -> the seconds digits are now most-significant first
    emitter.instruction("mov cl, BYTE PTR [r9]");                               // load the left byte
    emitter.instruction("mov dl, BYTE PTR [r10]");                              // load the right byte
    emitter.instruction("mov BYTE PTR [r9], dl");                               // write the right byte to the left position
    emitter.instruction("mov BYTE PTR [r10], cl");                              // write the left byte to the right position
    emitter.instruction("inc r9");                                              // advance the left pointer
    emitter.instruction("dec r10");                                             // retreat the right pointer
    emitter.instruction("jmp __rt_microtime_build_into_rev_loop");              // continue swapping until the pointers meet
    emitter.label("__rt_microtime_build_into_done");
    emitter.instruction("mov rax, rsi");                                        // rax = current write position
    emitter.instruction("sub rax, rdi");                                        // rax = bytes written (position minus buffer start)

    // -- tear down stack frame --
    emitter.instruction("add rsp, 48");                                         // release the timeval, buffer save, and scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte count in rax
}

/// __rt_microtime_str: build the `microtime()` string form and return an owned copy.
/// No arguments. Output: x1 = owned string pointer, x2 = length (string result ABI).
/// Builds the "0.NNNNNNNN sec" text on a stack scratch and persists it into a refcount-owned
/// heap string via __rt_str_persist, so the caller's normal string cleanup frees it correctly.
pub(crate) fn emit_microtime_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_microtime_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: microtime_str ---");
    emitter.label_global("__rt_microtime_str");

    // -- set up stack frame: [sp,#0..32] string scratch, [sp,#32..48] frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate the 32-byte string scratch and the saved frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set the new frame pointer

    // -- build the string form into the stack scratch --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to the 32-byte stack scratch
    emitter.instruction("bl __rt_microtime_build_into");                        // write the microtime string into the scratch, x0 = length

    // -- persist the stack scratch into an owned heap string --
    emitter.instruction("mov x2, x0");                                          // x2 = string length (str_persist length register)
    emitter.instruction("add x1, sp, #0");                                      // x1 = scratch pointer (str_persist pointer register)
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned string pointer, x2 = length

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the owned string in x1/x2
}

/// x86_64 Linux variant of `__rt_microtime_str`.
/// No arguments. Output: rax = owned string pointer, rdx = length (string result ABI).
fn emit_microtime_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: microtime_str ---");
    emitter.label_global("__rt_microtime_str");

    // -- set up stack frame: [rsp+0..32] string scratch, [rsp+32..48] alignment pad --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the string scratch
    emitter.instruction("sub rsp, 48");                                         // reserve the 32-byte scratch plus alignment padding (16-aligned for calls)

    // -- build the string form into the stack scratch --
    emitter.instruction("lea rdi, [rsp]");                                      // rdi = pointer to the 32-byte stack scratch
    emitter.instruction("call __rt_microtime_build_into");                      // write the microtime string into the scratch, rax = length

    // -- persist the stack scratch into an owned heap string --
    emitter.instruction("mov rdx, rax");                                        // rdx = string length (str_persist length register)
    emitter.instruction("lea rax, [rsp]");                                      // rax = scratch pointer (str_persist pointer register)
    emitter.instruction("call __rt_str_persist");                               // rax = owned string pointer, rdx = length

    // -- tear down stack frame --
    emitter.instruction("add rsp, 48");                                         // release the scratch and alignment padding
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned string in rax/rdx
}

/// __rt_microtime_mixed: box the `microtime($flag)` result as a Mixed cell.
/// Input:  x0 = as_float flag (0 = string form, nonzero = float form).
/// Output: x0 = boxed Mixed cell pointer.
/// Branches on the flag: the float form reuses __rt_microtime and boxes tag 2; the string form
/// builds the "0.NNNNNNNN sec" text on a stack scratch and boxes tag 1, which __rt_mixed_from_value
/// persists into an owned heap copy (the stack scratch is reclaimed on return, so no leak).
pub(crate) fn emit_microtime_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_microtime_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: microtime_mixed ---");
    emitter.label_global("__rt_microtime_mixed");

    // -- set up stack frame: [sp,#0..32] string scratch, [sp,#40..48] flag, [sp,#48..64] frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate the string scratch, flag save, and saved frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set the new frame pointer
    emitter.instruction("str x0, [sp, #40]");                                   // save the as_float flag across the helper calls

    // -- branch on the flag: nonzero -> float form, zero -> string form --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the flag into a scratch register
    emitter.instruction("cbz x9, __rt_microtime_mixed_str");                    // flag = false -> build the string form

    // -- float form: reuse the float helper and box the double as tag 2 --
    emitter.instruction("bl __rt_microtime");                                   // d0 = seconds.microseconds as a double
    emitter.instruction("fmov x1, d0");                                         // pass the float bits as the Mixed payload low word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = float
    emitter.instruction("mov x2, #0");                                          // float payloads do not use the high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed Mixed cell pointer
    emitter.instruction("b __rt_microtime_mixed_done");                         // skip the string-form path

    // -- string form: build the text on the stack and box tag 1 --
    emitter.label("__rt_microtime_mixed_str");
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to the 32-byte stack scratch
    emitter.instruction("bl __rt_microtime_build_into");                        // write the microtime string into the scratch, x0 = length
    emitter.instruction("mov x2, x0");                                          // x2 = string length (Mixed payload high word)
    emitter.instruction("add x1, sp, #0");                                      // x1 = scratch pointer (Mixed payload low word)
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed Mixed cell pointer (persists the string)

    // -- tear down stack frame --
    emitter.label("__rt_microtime_mixed_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in x0
}

/// x86_64 Linux variant of `__rt_microtime_mixed`.
/// Input:  rdi = as_float flag (0 = string form, nonzero = float form).
/// Output: rax = boxed Mixed cell pointer.
fn emit_microtime_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: microtime_mixed ---");
    emitter.label_global("__rt_microtime_mixed");

    // -- set up stack frame: [rsp+0..32] string scratch, [rsp+32..40] flag, [rsp+40..48] pad --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the scratch and flag
    emitter.instruction("sub rsp, 48");                                         // reserve the scratch, flag save, and alignment padding (16-aligned for calls)
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // save the as_float flag across the helper calls

    // -- branch on the flag: nonzero -> float form, zero -> string form --
    emitter.instruction("test rdi, rdi");                                       // is the flag false (zero)?
    emitter.instruction("jz __rt_microtime_mixed_str");                         // flag = false -> build the string form

    // -- float form: reuse the float helper and box the double as tag 2 --
    emitter.instruction("call __rt_microtime");                                 // xmm0 = seconds.microseconds as a double
    emitter.instruction("movq rdi, xmm0");                                      // pass the float bits as the Mixed payload low word
    emitter.instruction("mov rax, 2");                                          // runtime tag 2 = float
    emitter.instruction("xor esi, esi");                                        // float payloads do not use the high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed Mixed cell pointer
    emitter.instruction("jmp __rt_microtime_mixed_done");                       // skip the string-form path

    // -- string form: build the text on the stack and box tag 1 --
    emitter.label("__rt_microtime_mixed_str");
    emitter.instruction("lea rdi, [rsp]");                                      // rdi = pointer to the 32-byte stack scratch
    emitter.instruction("call __rt_microtime_build_into");                      // write the microtime string into the scratch, rax = length
    emitter.instruction("mov rsi, rax");                                        // rsi = string length (Mixed payload high word)
    emitter.instruction("lea rdi, [rsp]");                                      // rdi = scratch pointer (Mixed payload low word)
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed Mixed cell pointer (persists the string)

    // -- tear down stack frame --
    emitter.label("__rt_microtime_mixed_done");
    emitter.instruction("add rsp, 48");                                         // release the scratch, flag save, and padding
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed Mixed pointer in rax
}
