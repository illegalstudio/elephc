//! Purpose:
//! Emits the `__rt_fread`, `__rt_fread_done` runtime helper assembly for fread.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.
//! - The destination window is reserved through `__rt_concat_reserve` for the FULL requested
//!   read length before the syscall, so an attacker-sized `fread($f, 100000)` lands in owned
//!   heap storage instead of running past the 64 KiB concat scratch into the stream-handle,
//!   exception and heap globals that follow it in BSS.

use elephc_memoir::buffers::{stream_scratch, stream_tables};
use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;

/// Emits the `__rt_fread` runtime helper for reading bytes from a file descriptor.
///
/// On ARM64: reads into storage reserved by `__rt_concat_reserve`, publishes the bytes read
/// through `__rt_concat_publish`, sets `_eof_flags[fd]` on EOF, and returns
/// (pointer, byte_count) in x1:x2.
///
/// On x86_64: same semantics but uses libc `read()` and returns (pointer, byte_count) in rax:rdx.
///
/// # Inputs
/// - x0/rdi: file descriptor
/// - x1/rsi: number of bytes to read
///
/// # Outputs
/// - x1/x86_64 rax: pointer to the bytes read. Concat-scratch-backed (borrowed) when the
///   requested length still fits the shared 64 KiB buffer, heap-backed otherwise.
/// - x2/rdx: actual bytes read (0 on EOF/error)
///
/// # Side effects
/// - Advances `_concat_off` by the actual bytes read, but only for scratch-backed results.
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
    emitter.instruction("b.lt __rt_fread_real_fd");                             // not a wrapper fd → issue the real read syscall path
    emitter.instruction("b __rt_user_wrapper_fread");                           // wrapper fd: tail-call stream_read (uncond → cross-atom safe)
    emitter.label("__rt_fread_real_fd");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and requested length --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length

    // -- reserve a destination sized for the whole requested read --
    emitter.instruction("cmp x1, #1");                                          // does the caller actually request at least one byte?
    emitter.instruction("b.lt __rt_fread_dest_scratch");                        // non-positive requests write nothing, so keep the current scratch tail
    emitter.instruction("cmp x0, #256");                                        // only registered descriptors can carry a read filter
    emitter.instruction("b.hs __rt_fread_size_ready");                          // invalid or untracked descriptors use the raw read length
    stream_tables::READ_FILTERS.emit_load_indexed_byte(emitter, "w9", "x0", "x9");
    emitter.instruction("cmp w9, #6");                                          // is this the Base64 expansion filter?
    emitter.instruction("b.eq __rt_fread_expand_size");                         // reserve additional space for padded output
    emitter.instruction("cmp w9, #8");                                          // is this the quoted-printable expansion filter?
    emitter.instruction("b.ne __rt_fread_size_ready");                          // other builtins never grow the caller's read window
    emitter.label("__rt_fread_expand_size");
    emitter.instruction("mov x0, x1");                                          // pass the original read count to the capacity planner
    stream_scratch::emit_growing_read_capacity(emitter, "__rt_fread_expand");
    emitter.instruction("b __rt_fread_reserve");                                // use the larger of the read and transformed lengths
    emitter.label("__rt_fread_size_ready");
    emitter.instruction("mov x0, x1");                                          // reserve exactly the caller's requested read size
    emitter.label("__rt_fread_reserve");
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the incoming bytes
    emitter.instruction("mov x12, x0");                                         // destination pointer for the read
    emitter.instruction("b __rt_fread_dest_ready");                             // the destination window is reserved
    emitter.label("__rt_fread_dest_scratch");
    elephc_memoir::strings::concat_cursor::emit_cursor_address(emitter, "x9");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    elephc_memoir::strings::concat_cursor::emit_base_address(emitter, "x11");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.label("__rt_fread_dest_ready");
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- TLS dispatch: route through elephc_tls_read when fd has an
    //    attached session (Phase 11 B3). --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for the TLS check
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_tls_sessions");
    emitter.instruction("ldr x14, [x13, x0, lsl #3]");                          // _tls_sessions[fd] handle (0 = plain TCP)
    emitter.instruction("cbz x14, __rt_fread_do_syscall");                      // no TLS attached → fall through to read syscall
    emitter.instruction("mov x0, x14");                                         // handle as first arg
    emitter.instruction("ldr x1, [sp, #16]");                                   // buf ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load elephc_tls_read entry pointer
    emitter.instruction("blr x9");                                              // x0 = bytes read (>=0) or -1
    emitter.instruction("cmp x0, #0");                                          // value-based check after the TLS call
    emitter.instruction("b.ge __rt_fread_read_ok");                             // continue when TLS read returned >= 0
    emitter.instruction("str xzr, [sp, #24]");                                  // TLS error: zero-length result
    emitter.instruction("b __rt_fread_mark_eof");                               // mark the stream exhausted

    emitter.label("__rt_fread_do_syscall");
    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd for read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // buffer pointer for read
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

    // -- publish the bytes actually read into the reserved destination --
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the reserved destination pointer
    emitter.instruction("mov x2, x0");                                          // pass the number of bytes actually read
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed reads

    // -- set eof flag if read returned 0 --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("cbnz x0, __rt_fread_done");                            // if bytes > 0, skip eof flag
    emitter.label("__rt_fread_mark_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("mov w10, #1");                                         // eof marker value
    elephc_memoir::buffers::stream_tables::EOF_FLAGS.emit_store_indexed_byte(emitter, "w10", "x0", "x9");

    emitter.label("__rt_fread_would_block");
    emitter.instruction("str xzr, [sp, #24]");                                  // return an empty read without setting EOF for EAGAIN/EWOULDBLOCK

    // -- return pointer and length --
    emitter.label("__rt_fread_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return actual bytes read as length

    // -- apply an attached read filter to the bytes just read --
    emitter.instruction("cbz x2, __rt_fread_ret");                              // EOF and transient empty reads have no bucket to filter
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    stream_tables::READ_FILTERS.emit_load_indexed_byte(emitter, "w3", "x0", "x9");
    emitter.instruction("cbz w3, __rt_fread_ret");                              // skip when no read filter is attached
    emitter.instruction("cmp w3, #128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("b.lt __rt_fread_builtin_filter");                      // built-in filter: in-place transform
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read) for the user-filter dispatch
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← user filter's transformed string
    emitter.instruction("b __rt_fread_ret");                                    // common epilogue
    emitter.label("__rt_fread_builtin_filter");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform the read bytes in place; x2 = (possibly compacted) length on return
    emitter.instruction("ldr x9, [sp, #24]");                                   // recover the original number of bytes read
    emitter.instruction("cmp x2, x9");                                          // did the filter extend the published range?
    emitter.instruction("b.ls __rt_fread_ret");                                 // preserve historical cursor placement for shrinking results
    emitter.instruction("bl __rt_concat_publish");                              // protect every byte of the length-growing result

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
    emitter.instruction("cmp rsi, 1");                                          // does the caller actually request at least one byte?
    emitter.instruction("jl __rt_fread_dest_scratch_x86");                      // non-positive requests write nothing, so keep the current scratch tail
    emitter.instruction("cmp rdi, 256");                                        // only registered descriptors can carry a read filter
    emitter.instruction("jae __rt_fread_size_ready_x86");                       // invalid or untracked descriptors use the raw read length
    stream_tables::READ_FILTERS.emit_load_indexed_byte(emitter, "ecx", "rdi", "r10");
    emitter.instruction("cmp ecx, 6");                                          // is this the Base64 expansion filter?
    emitter.instruction("je __rt_fread_expand_size_x86");                       // reserve additional space for padded output
    emitter.instruction("cmp ecx, 8");                                          // is this the quoted-printable expansion filter?
    emitter.instruction("jne __rt_fread_size_ready_x86");                       // other builtins never grow the caller's read window
    emitter.label("__rt_fread_expand_size_x86");
    emitter.instruction("mov rax, rsi");                                        // pass the original read count to the capacity planner
    stream_scratch::emit_growing_read_capacity(emitter, "__rt_fread_expand_x86");
    emitter.instruction("jmp __rt_fread_reserve_x86");                          // use the larger of the read and transformed lengths
    emitter.label("__rt_fread_size_ready_x86");
    emitter.instruction("mov rax, rsi");                                        // reserve exactly the caller's requested read size
    emitter.label("__rt_fread_reserve_x86");
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the incoming bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the reserved destination pointer for the final elephc string result
    emitter.instruction("jmp __rt_fread_dest_ready_x86");                       // the destination window is reserved
    emitter.label("__rt_fread_dest_scratch_x86");
    elephc_memoir::strings::concat_cursor::emit_read(emitter, "r10");             // load the current concat-buffer absolute offset before appending the fread() result
    elephc_memoir::strings::concat_cursor::emit_base_address(emitter, "r11");                    // materialize the concat-buffer base address once for the x86_64 fread() helper
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the start pointer for the bytes that libc read() will append
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the concat-buffer start pointer for the final elephc string result
    emitter.label("__rt_fread_dest_ready_x86");

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
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the original read count across filtering
    emitter.instruction("mov rdx, rax");                                        // return the successful byte count in the x86_64 elephc string-length result register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the reserved start pointer in the x86_64 elephc string-pointer result register
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed reads
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor for the read-filter lookup
    stream_tables::READ_FILTERS.emit_load_indexed_byte(emitter, "ecx", "r10", "r11");
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
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 32]");                       // did the filter extend the published range?
    emitter.instruction("jbe __rt_fread_ret_x86");                              // preserve historical cursor placement for shrinking results
    emitter.instruction("call __rt_concat_publish");                            // protect every byte of the length-growing result
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
    elephc_memoir::buffers::stream_tables::EOF_FLAGS.emit_store_indexed_byte(emitter, "1", "r10", "r11");
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when libc read() reports EOF or failure
    emitter.instruction("xor edx, edx");                                        // return an empty string length when libc read() reports EOF or failure
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the empty-string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the EOF/error fread() path
    emitter.instruction("ret");                                                 // return the empty string result for the exhausted or failed stream read
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// A growing filter has a checked destination before read and publishes the expanded output.
    #[test]
    fn fread_bounded_expansion_covers_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_fread(&mut emitter);
            let asm = emitter.output();
            let capacity = asm.find(if name == "linux-x86_64" {
                "__rt_fread_expand_x86_bounded:"
            } else {
                "__rt_fread_expand_bounded:"
            }).unwrap();
            let reserve = asm.find("__rt_concat_reserve").unwrap();
            let filtered = asm.find("__rt_apply_stream_filter").unwrap();
            let publish = asm.rfind("__rt_concat_publish").unwrap();
            assert!(capacity < reserve && reserve < filtered && filtered < publish, "{name}: {asm}");
        }
    }
}
