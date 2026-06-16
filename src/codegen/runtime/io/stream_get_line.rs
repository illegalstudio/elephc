//! Purpose:
//! Emits the `__rt_stream_get_line` runtime helper assembly for the
//! stream_get_line builtin.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Reads one byte at a time into the concat buffer until the byte budget is
//!   spent, EOF is reached, or the trailing bytes match the ending delimiter
//!   (which is consumed and stripped). EOF/read failure sets `_eof_flags`.

use crate::codegen::abi::emit_symbol_address;
use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

/// stream_get_line: read up to a length or an ending delimiter from a stream.
/// Input:  x0=fd, x1=max length, x2=ending pointer, x3=ending length
/// Output: x1=string pointer (in concat_buf), x2=length read (delimiter stripped)
pub fn emit_stream_get_line(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_line_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_get_line ---");
    emitter.label_global("__rt_stream_get_line");

    // Frame: [0..16) regs, [16) fd, [24) length, [32) ending ptr, [40) ending
    //        len, [48) result start, [56) running total.
    emitter.instruction("sub sp, sp, #64");                                     // frame for saved regs and parse state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the file descriptor
    emitter.instruction("str x1, [sp, #24]");                                   // save the maximum length
    emitter.instruction("str x2, [sp, #32]");                                   // save the ending-delimiter pointer
    emitter.instruction("str x3, [sp, #40]");                                   // save the ending-delimiter length

    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // result start pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save the result start pointer
    emitter.instruction("str xzr, [sp, #56]");                                  // running total starts at zero

    // -- user-wrapper fd: read via stream_read into _user_wrapper_drain_buf --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the file descriptor
    emitter.instruction("mov w9, #0x4000");                                     // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w9, w9, #16");                                     // form 0x40000000 in w9
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.ge __rt_sgl_wrapper_entry");                         // wrappers read via the feof-gated stream_read loop below

    emitter.label("__rt_stream_get_line_loop");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("ldr x11, [sp, #24]");                                  // maximum length
    emitter.instruction("cmp x10, x11");                                        // reached the byte budget?
    emitter.instruction("b.ge __rt_stream_get_line_done");                      // stop at the maximum length

    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x1, x11, x10");                                    // single-byte write pointer
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the file descriptor
    emitter.instruction("mov x2, #1");                                          // read exactly one byte
    emitter.syscall(3);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_get_line_read_ok")); // continue when the read succeeded
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", plat.would_block_errno())); // Linux: compare read result with -EAGAIN/-EWOULDBLOCK
    } else {
        emitter.instruction(&format!("cmp x0, #{}", plat.would_block_errno())); // macOS: compare errno with EAGAIN/EWOULDBLOCK
    }
    emitter.instruction("b.eq __rt_stream_get_line_done");                      // transient nonblocking miss is not EOF
    emitter.instruction("b __rt_stream_get_line_eof");                          // a read failure ends the line
    emitter.label("__rt_stream_get_line_read_ok");
    emitter.instruction("cbz x0, __rt_stream_get_line_eof");                    // a zero-byte read means EOF

    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // concat-buffer offset
    emitter.instruction("add x10, x10, #1");                                    // advance past the byte just read
    emitter.instruction("str x10, [x9]");                                       // publish the updated offset
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("add x10, x10, #1");                                    // count the new byte
    emitter.instruction("str x10, [sp, #56]");                                  // store the running total

    // -- check whether the trailing bytes match the ending delimiter --
    emitter.instruction("ldr x3, [sp, #40]");                                   // ending-delimiter length
    emitter.instruction("cbz x3, __rt_stream_get_line_loop");                   // no delimiter: keep reading
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("cmp x10, x3");                                         // enough bytes for a delimiter match?
    emitter.instruction("b.lt __rt_stream_get_line_loop");                      // not yet: keep reading
    emitter.instruction("ldr x12, [sp, #48]");                                  // result start pointer
    emitter.instruction("sub x13, x10, x3");                                    // offset of the candidate tail
    emitter.instruction("add x13, x12, x13");                                   // pointer to the candidate tail
    emitter.instruction("ldr x14, [sp, #32]");                                  // ending-delimiter pointer
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index
    emitter.label("__rt_stream_get_line_cmp");
    emitter.instruction("cmp x15, x3");                                         // compared every delimiter byte?
    emitter.instruction("b.ge __rt_stream_get_line_matched");                   // a full match ends the line
    emitter.instruction("ldrb w16, [x13, x15]");                                // a tail byte
    emitter.instruction("ldrb w17, [x14, x15]");                                // the matching delimiter byte
    emitter.instruction("cmp w16, w17");                                        // do they differ?
    emitter.instruction("b.ne __rt_stream_get_line_loop");                      // mismatch: keep reading
    emitter.instruction("add x15, x15, #1");                                    // advance the comparison
    emitter.instruction("b __rt_stream_get_line_cmp");                          // compare the next delimiter byte

    emitter.label("__rt_stream_get_line_matched");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("sub x10, x10, x3");                                    // drop the delimiter from the result
    emitter.instruction("str x10, [sp, #56]");                                  // store the stripped total
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // concat-buffer offset
    emitter.instruction("sub x10, x10, x3");                                    // rewind past the consumed delimiter
    emitter.instruction("str x10, [x9]");                                       // publish the rewound offset
    emitter.instruction("b __rt_stream_get_line_done");                         // a delimiter match is not EOF

    // -- user-wrapper line read: feof-gated stream_read into _user_wrapper_drain_buf
    //    (a SEPARATE buffer from _concat_buf, which each __rt_fread result may
    //    occupy). [sp,#48] = drain-buf base, [sp,#56] = running length. Stops at
    //    the byte budget, the ending delimiter (stripped), or EOF. --
    emitter.label("__rt_sgl_wrapper_entry");
    emit_symbol_address(emitter, "x12", "_user_wrapper_drain_buf");
    emitter.instruction("str x12, [sp, #48]");                                  // result start = drain-buf base
    emitter.label("__rt_sgl_wrapper_loop");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("ldr x11, [sp, #24]");                                  // maximum length
    emitter.instruction("cmp x10, x11");                                        // reached the byte budget?
    emitter.instruction("b.ge __rt_stream_get_line_done");                      // stop at the maximum length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the wrapper fd
    emitter.instruction("bl __rt_feof");                                        // check stream_eof FIRST (x0 = 1 at EOF)
    emitter.instruction("cbnz x0, __rt_stream_get_line_done");                  // at EOF: return the bytes gathered so far
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the wrapper fd
    emitter.instruction("mov x1, #1");                                          // read exactly one byte
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_stream_get_line_done");                   // defensive: empty read also ends the line
    emitter.instruction("ldrb w13, [x1]");                                      // load the read byte
    emitter.instruction("ldr x10, [sp, #56]");                                  // current running total
    emitter.instruction("ldr x12, [sp, #48]");                                  // drain-buf base
    emitter.instruction("strb w13, [x12, x10]");                                // append the byte to the line buffer
    emitter.instruction("add x10, x10, #1");                                    // advance the running total
    emitter.instruction("str x10, [sp, #56]");                                  // store the updated total
    emitter.instruction("mov x0, x1");                                          // chunk ptr (byte already copied)
    emitter.instruction("bl __rt_decref_any");                                  // release the owned chunk
    emitter.instruction("ldr x3, [sp, #40]");                                   // ending-delimiter length
    emitter.instruction("cbz x3, __rt_sgl_wrapper_loop");                       // no delimiter: keep reading
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("cmp x10, x3");                                         // enough bytes for a delimiter match?
    emitter.instruction("b.lt __rt_sgl_wrapper_loop");                          // not yet: keep reading
    emitter.instruction("ldr x12, [sp, #48]");                                  // drain-buf base
    emitter.instruction("sub x13, x10, x3");                                    // offset of the candidate tail
    emitter.instruction("add x13, x12, x13");                                   // pointer to the candidate tail
    emitter.instruction("ldr x14, [sp, #32]");                                  // ending-delimiter pointer
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index
    emitter.label("__rt_sgl_wrapper_cmp");
    emitter.instruction("cmp x15, x3");                                         // compared every delimiter byte?
    emitter.instruction("b.ge __rt_sgl_wrapper_matched");                       // a full match ends the line
    emitter.instruction("ldrb w16, [x13, x15]");                                // a tail byte
    emitter.instruction("ldrb w17, [x14, x15]");                                // the matching delimiter byte
    emitter.instruction("cmp w16, w17");                                        // do they differ?
    emitter.instruction("b.ne __rt_sgl_wrapper_loop");                          // mismatch: keep reading
    emitter.instruction("add x15, x15, #1");                                    // advance the comparison
    emitter.instruction("b __rt_sgl_wrapper_cmp");                              // compare the next delimiter byte
    emitter.label("__rt_sgl_wrapper_matched");
    emitter.instruction("ldr x10, [sp, #56]");                                  // running total
    emitter.instruction("sub x10, x10, x3");                                    // drop the delimiter from the result
    emitter.instruction("str x10, [sp, #56]");                                  // store the stripped total
    emitter.instruction("b __rt_stream_get_line_done");                         // a delimiter match is not EOF

    emitter.label("__rt_stream_get_line_eof");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the file descriptor
    emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // EOF marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // record EOF for this descriptor

    emitter.label("__rt_stream_get_line_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // return the result start pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // return the bytes read
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the line slice
}

/// Emits the Linux x86_64 stream runtime helper for stream get line.
fn emit_stream_get_line_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_line ---");
    emitter.label_global("__rt_stream_get_line");

    // Frame: [rbp-8) fd, [rbp-16) length, [rbp-24) ending ptr, [rbp-32) ending
    //        len, [rbp-40) result start, [rbp-48) running total.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the parse state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the maximum length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the ending-delimiter pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the ending-delimiter length

    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // current concat-buffer offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");                    // concat-buffer base address
    emitter.instruction("lea r11, [r10 + r9]");                                 // result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // running total starts at zero

    // -- user-wrapper fd: read via stream_read into _user_wrapper_drain_buf --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_sgl_wrapper_entry_x86");                      // wrappers read via the feof-gated stream_read loop below

    emitter.label("__rt_stream_get_line_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // reached the byte budget?
    emitter.instruction("jge __rt_stream_get_line_done_x86");                   // stop at the maximum length

    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // current concat-buffer offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");                    // concat-buffer base address
    emitter.instruction("lea rsi, [r10 + r9]");                                 // single-byte write pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emitter.instruction("mov rdx, 1");                                          // read exactly one byte
    emitter.instruction("call read");                                           // read one byte through libc read()
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as a byte, EOF, or failure
    emitter.instruction("jg __rt_stream_get_line_read_ok_x86");                 // positive byte count: publish the appended byte
    emitter.instruction("jl __rt_stream_get_line_read_failed_x86");             // negative result: inspect errno before setting EOF
    emitter.instruction("jmp __rt_stream_get_line_eof_x86");                    // zero-byte read means real EOF

    emitter.label("__rt_stream_get_line_read_ok_x86");
    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // concat-buffer offset
    emitter.instruction("inc r9");                                              // advance past the byte just read
    abi::emit_store_reg_to_symbol(emitter, "r9", "_concat_off", 0);             // publish the updated offset
    emitter.instruction("inc QWORD PTR [rbp - 48]");                            // count the new byte

    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // ending-delimiter length
    emitter.instruction("test rcx, rcx");                                       // no delimiter configured?
    emitter.instruction("jz __rt_stream_get_line_loop_x86");                    // keep reading without a delimiter
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, rcx");                                        // enough bytes for a delimiter match?
    emitter.instruction("jl __rt_stream_get_line_loop_x86");                    // not yet: keep reading
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // result start pointer
    emitter.instruction("mov r9, rax");                                         // running total
    emitter.instruction("sub r9, rcx");                                         // offset of the candidate tail
    emitter.instruction("add r8, r9");                                          // pointer to the candidate tail
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // ending-delimiter pointer
    emitter.instruction("xor esi, esi");                                        // delimiter comparison index
    emitter.label("__rt_stream_get_line_cmp_x86");
    emitter.instruction("cmp rsi, rcx");                                        // compared every delimiter byte?
    emitter.instruction("jge __rt_stream_get_line_matched_x86");                // a full match ends the line
    emitter.instruction("movzx edi, BYTE PTR [r8 + rsi]");                      // a tail byte
    emitter.instruction("movzx edx, BYTE PTR [r10 + rsi]");                     // the matching delimiter byte
    emitter.instruction("cmp edi, edx");                                        // do they differ?
    emitter.instruction("jne __rt_stream_get_line_loop_x86");                   // mismatch: keep reading
    emitter.instruction("inc rsi");                                             // advance the comparison
    emitter.instruction("jmp __rt_stream_get_line_cmp_x86");                    // compare the next delimiter byte

    emitter.label("__rt_stream_get_line_matched_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("sub rax, rcx");                                        // drop the delimiter from the result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // store the stripped total
    abi::emit_load_symbol_to_reg(emitter, "r9", "_concat_off", 0);              // concat-buffer offset
    emitter.instruction("sub r9, rcx");                                         // rewind past the consumed delimiter
    abi::emit_store_reg_to_symbol(emitter, "r9", "_concat_off", 0);             // publish the rewound offset
    emitter.instruction("jmp __rt_stream_get_line_done_x86");                   // a delimiter match is not EOF

    emitter.label("__rt_stream_get_line_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_stream_get_line_done_x86");                    // transient nonblocking miss returns without setting EOF

    // -- user-wrapper line read: feof-gated stream_read into _user_wrapper_drain_buf
    //    (a SEPARATE buffer from _concat_buf, which each __rt_fread result may
    //    occupy). [rbp-40] = drain-buf base, [rbp-48] = running length. --
    emitter.label("__rt_sgl_wrapper_entry_x86");
    abi::emit_symbol_address(emitter, "r10", "_user_wrapper_drain_buf");        // drain-buf base
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // result start = drain-buf base
    emitter.label("__rt_sgl_wrapper_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // reached the byte budget?
    emitter.instruction("jge __rt_stream_get_line_done_x86");                   // stop at the maximum length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper fd
    emitter.instruction("call __rt_feof");                                      // check stream_eof FIRST (rax = 1 at EOF)
    emitter.instruction("test rax, rax");                                       // at EOF?
    emitter.instruction("jnz __rt_stream_get_line_done_x86");                   // at EOF: return the bytes gathered so far
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper fd
    emitter.instruction("mov rsi, 1");                                          // read exactly one byte
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");                                       // zero-length read?
    emitter.instruction("jz __rt_stream_get_line_done_x86");                    // defensive: empty read also ends the line
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the read byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // current running total
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // drain-buf base
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // append the byte to the line buffer
    emitter.instruction("inc r10");                                             // advance the running total
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // store the updated total
    emitter.instruction("call __rt_decref_any");                                // release the owned chunk (rax = chunk ptr)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // ending-delimiter length
    emitter.instruction("test rcx, rcx");                                       // no delimiter configured?
    emitter.instruction("jz __rt_sgl_wrapper_loop_x86");                        // keep reading without a delimiter
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("cmp rax, rcx");                                        // enough bytes for a delimiter match?
    emitter.instruction("jl __rt_sgl_wrapper_loop_x86");                        // not yet: keep reading
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // drain-buf base
    emitter.instruction("mov r9, rax");                                         // running total
    emitter.instruction("sub r9, rcx");                                         // offset of the candidate tail
    emitter.instruction("add r8, r9");                                          // pointer to the candidate tail
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // ending-delimiter pointer
    emitter.instruction("xor esi, esi");                                        // delimiter comparison index
    emitter.label("__rt_sgl_wrapper_cmp_x86");
    emitter.instruction("cmp rsi, rcx");                                        // compared every delimiter byte?
    emitter.instruction("jge __rt_sgl_wrapper_matched_x86");                    // a full match ends the line
    emitter.instruction("movzx edi, BYTE PTR [r8 + rsi]");                      // a tail byte
    emitter.instruction("movzx edx, BYTE PTR [r10 + rsi]");                     // the matching delimiter byte
    emitter.instruction("cmp edi, edx");                                        // do they differ?
    emitter.instruction("jne __rt_sgl_wrapper_loop_x86");                       // mismatch: keep reading
    emitter.instruction("inc rsi");                                             // advance the comparison
    emitter.instruction("jmp __rt_sgl_wrapper_cmp_x86");                        // compare the next delimiter byte
    emitter.label("__rt_sgl_wrapper_matched_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // running total
    emitter.instruction("sub rax, rcx");                                        // drop the delimiter from the result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // store the stripped total
    emitter.instruction("jmp __rt_stream_get_line_done_x86");                   // a delimiter match is not EOF

    emitter.label("__rt_stream_get_line_eof_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the file descriptor
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");                     // eof-flag table base address
    emitter.instruction("mov BYTE PTR [r10 + r9], 1");                          // record EOF for this descriptor

    emitter.label("__rt_stream_get_line_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the result start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // return the bytes read
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the line slice
}
