//! Purpose:
//! Emits the `__rt_fread`, `__rt_fread_done` runtime helper assembly for fread.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::codegen::abi;

/// Emits the `__rt_fread` runtime helper for reading bytes from a file descriptor.
///
/// On ARM64: reads into the concat buffer, updates `_concat_off`, sets `_eof_flags[fd]` on EOF,
/// and returns (pointer, byte_count) in x1:x2.
///
/// On x86_64: same semantics but uses libc `read()` and returns (pointer, byte_count) in rax:rdx.
///
/// # Inputs
/// - x0/rdi: file descriptor
/// - x1/rsi: number of bytes to read
///
/// # Outputs
/// - x1/x86_64 rax: pointer to bytes in concat buffer (borrowed, not owned)
/// - x2/rdx: actual bytes read (0 on EOF/error)
///
/// # Side effects
/// - Advances `_concat_off` by actual bytes read.
/// - Sets `_eof_flags[fd] = 1` when the stream is exhausted.
pub fn emit_fread(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fread_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.ge __rt_user_wrapper_fread");                        // dispatch into the wrapper's stream_read instead of issuing a read syscall

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and requested length --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- TLS dispatch: route through elephc_tls_read when fd has an
    //    attached session (Phase 11 B3). --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for the TLS check
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_tls_sessions");
    emitter.instruction("ldr x14, [x13, x0, lsl #3]");                          // _tls_sessions[fd] handle (0 = plain TCP)
    emitter.instruction("cbz x14, __rt_fread_do_syscall");                      // no TLS attached → fall through to read syscall
    emitter.instruction("mov x0, x14");                                         // handle as first arg
    emitter.instruction("mov x1, x12");                                         // buf ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load elephc_tls_read entry pointer
    emitter.instruction("blr x9");                                              // x0 = bytes read (>=0) or -1
    emitter.instruction("cmp x0, #0");                                          // value-based check after the TLS call
    emitter.instruction("b.ge __rt_fread_read_ok");                             // continue when TLS read returned >= 0
    emitter.instruction("str xzr, [sp, #24]");                                  // TLS error: zero-length result
    emitter.instruction("b __rt_fread_mark_eof");                               // mark the stream exhausted

    emitter.label("__rt_fread_do_syscall");
    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd for read syscall
    emitter.instruction("mov x1, x12");                                         // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // number of bytes to read
    emitter.syscall(3);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: negative read result means failure
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fread_read_ok")); // continue only when the read syscall succeeded
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", emitter.platform.would_block_errno())); // Linux: is this -EAGAIN/-EWOULDBLOCK from a nonblocking fd?
    } else {
        emitter.instruction(&format!("cmp x0, #{}", emitter.platform.would_block_errno())); // macOS: is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    }
    emitter.instruction("b.eq __rt_fread_would_block");                         // a transient nonblocking miss is not EOF
    emitter.instruction("str xzr, [sp, #24]");                                  // failed reads return an empty result
    emitter.instruction("b __rt_fread_mark_eof");                               // mark the stream as exhausted after a read failure
    emitter.label("__rt_fread_read_ok");

    // -- update concat_off by actual bytes read --
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x0");                                    // advance offset by bytes read
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set eof flag if read returned 0 --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("cbnz x0, __rt_fread_done");                            // if bytes > 0, skip eof flag
    emitter.label("__rt_fread_mark_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // eof marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1

    emitter.label("__rt_fread_would_block");
    emitter.instruction("str xzr, [sp, #24]");                                  // return an empty read without setting EOF for EAGAIN/EWOULDBLOCK

    // -- return pointer and length --
    emitter.label("__rt_fread_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return actual bytes read as length

    // -- apply an attached read filter to the bytes just read --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    emitter.instruction("ldrb w3, [x9, x0]");                                   // read filter id for this descriptor
    emitter.instruction("cbz w3, __rt_fread_ret");                              // skip when no read filter is attached
    emitter.instruction("cmp w3, #128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("b.lt __rt_fread_builtin_filter");                      // built-in filter: in-place transform
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read) for the user-filter dispatch
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← user filter's transformed string
    emitter.instruction("b __rt_fread_ret");                                    // common epilogue
    emitter.label("__rt_fread_builtin_filter");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform the read bytes in place; x2 = (possibly compacted) length on return

    // -- restore frame and return --
    emitter.label("__rt_fread_ret");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_fread` using libc `read()`.
fn emit_fread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_user_wrapper_fread");                         // dispatch into the wrapper's stream_read instead of issuing a read syscall

    emitter.instruction("cmp rdi, 0");                                          // does fread() have a valid non-negative file descriptor to read from?
    emitter.instruction("jge __rt_fread_fd_ok_x86");                            // continue to the normal read path when the file descriptor is valid
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer immediately when fopen() failed
    emitter.instruction("xor edx, edx");                                        // return an empty string length immediately when fopen() failed
    emitter.instruction("ret");                                                 // skip the stream read path entirely for invalid file descriptors

    emitter.label("__rt_fread_fd_ok_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fread() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor, length, and concat-buffer start pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the fread() read-path temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the file descriptor across the concat-buffer address computation and libc read() call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested byte count across the concat-buffer address computation and libc read() call
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // load the current concat-buffer absolute offset before appending the fread() result
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");                    // materialize the concat-buffer base address once for the x86_64 fread() helper
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the start pointer for the bytes that libc read() will append
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the concat-buffer start pointer for the final elephc string result

    // -- TLS dispatch: route through elephc_tls_read when fd has an
    //    attached session (Phase 11 B3). --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload fd for the TLS table lookup
    abi::emit_symbol_address(emitter, "r11", "_tls_sessions");                  // load runtime data address
    emitter.instruction("mov r12, QWORD PTR [r11 + r10 * 8]");                  // _tls_sessions[fd] handle (0 = plain TCP)
    emitter.instruction("test r12, r12");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_fread_do_syscall_x86");                        // no TLS attached → use libc read
    emitter.instruction("mov rdi, r12");                                        // handle as first arg
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // buf ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_read_fn", 0);      // prepare SysV call argument
    emitter.instruction("call r9");                                             // rax = bytes read (>=0) or -1
    emitter.instruction("cmp rax, 0");                                          // did the TLS bridge return bytes?
    emitter.instruction("jle __rt_fread_eof_x86");                              // TLS errors and EOF still mark the stream as exhausted
    emitter.instruction("jmp __rt_fread_read_ok_x86");                          // publish the successful TLS read
    emitter.label("__rt_fread_do_syscall_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the file descriptor as the first libc read() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the concat-buffer write pointer as the second libc read() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the requested byte count as the third libc read() argument
    emitter.instruction("call read");                                           // read the requested bytes into the concat-buffer append window through libc read()
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as bytes, EOF, or failure
    emitter.instruction("jg __rt_fread_read_ok_x86");                           // positive byte count: publish the successful read
    emitter.instruction("jl __rt_fread_read_failed_x86");                       // negative result: inspect errno before treating it as EOF
    emitter.instruction("jmp __rt_fread_eof_x86");                              // zero-byte read means real EOF

    emitter.label("__rt_fread_read_ok_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // reload the previous concat-buffer absolute offset before publishing the fread() append
    emitter.instruction("add r10, rax");                                        // advance the concat-buffer offset by the number of bytes libc read() returned
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // publish the updated concat-buffer offset for later string appenders
    emitter.instruction("mov rdx, rax");                                        // return the successful byte count in the x86_64 elephc string-length result register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer in the x86_64 elephc string-pointer result register
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor for the read-filter lookup
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // materialize the read-filter table base
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // read filter id for this descriptor
    emitter.instruction("test rcx, rcx");                                       // is a read filter attached to this stream?
    emitter.instruction("jz __rt_fread_ret_x86");                               // skip when no read filter is attached
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("jl __rt_fread_builtin_filter_x86");                    // built-in filter: in-place transform
    emitter.instruction("mov rdi, r10");                                        // fd into the user-filter dispatcher's first arg
    emitter.instruction("mov rsi, rax");                                        // buf ptr into the dispatcher's second arg
    // rdx already holds the byte count
    emitter.instruction("xor ecx, ecx");                                        // direction = 0 (read) for the user-filter dispatch
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← user filter's transformed string
    emitter.instruction("jmp __rt_fread_ret_x86");                              // common epilogue
    emitter.label("__rt_fread_builtin_filter_x86");
    emitter.instruction("call __rt_apply_stream_filter");                       // transform the read bytes in place; rdx = (possibly compacted) length on return
    emitter.label("__rt_fread_ret_x86");
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the successful string slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the successful fread() path
    emitter.instruction("ret");                                                 // return the borrowed concat-buffer string slice to the caller

    emitter.label("__rt_fread_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_fread_would_block_x86");                       // transient nonblocking miss returns empty without EOF
    emitter.instruction("jmp __rt_fread_eof_x86");                              // other read failures behave like an exhausted stream

    emitter.label("__rt_fread_would_block_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer for an empty transient read
    emitter.instruction("xor edx, edx");                                        // return a zero-length read result without setting EOF
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the empty string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the would-block fread() path
    emitter.instruction("ret");                                                 // return the empty non-EOF read result

    emitter.label("__rt_fread_eof_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor so the eof-flag table can mark this stream as exhausted
    abi::emit_symbol_address(emitter, "r11", "_eof_flags");                     // materialize the eof-flag table base address for the current stream descriptor
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // mark the current file descriptor as EOF-reached after the zero-byte or failed read
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when libc read() reports EOF or failure
    emitter.instruction("xor edx, edx");                                        // return an empty string length when libc read() reports EOF or failure
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the empty-string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the EOF/error fread() path
    emitter.instruction("ret");                                                 // return the empty string result for the exhausted or failed stream read
}
