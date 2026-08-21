//! Purpose:
//! Emits `__rt_fread_raw`, the unbuffered read behind `fread()`.
//!
//! `__rt_fread` itself lives in `fread_filtered`: it wraps this helper with the per-stream
//! filtered-read buffer PHP keeps, and tail-calls straight here when the stream has no read
//! filter chain. This helper therefore does NOT apply that chain — the wrapper does, because it
//! is the only place that can re-read when a filter withholds its output.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native reads resolve their descriptor from an opaque stream handle and
//!   publish EOF on the corresponding `StreamState`, including seekable short reads.
//! - The destination window is reserved through `__rt_concat_reserve` for the FULL requested
//!   read length before the syscall, so an attacker-sized `fread($f, 100000)` lands in owned
//!   heap storage instead of running past the 64 KiB concat scratch into the stream-handle,
//!   exception and heap globals that follow it in BSS.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;

/// Emits the `__rt_fread` runtime helper for reading bytes from a stream handle.
///
/// On ARM64: reads into storage reserved by `__rt_concat_reserve`, publishes the bytes read
/// through `__rt_concat_publish`, sets StreamState EOF, and returns (pointer, byte_count)
/// in x1:x2.
///
/// On x86_64: same semantics but uses libc `read()` and returns (pointer, byte_count) in rax:rdx.
///
/// # Inputs
/// - x0/rdi: opaque stream handle
/// - x1/rsi: number of bytes to read
///
/// # Outputs
/// - x1/x86_64 rax: pointer to the bytes read. Concat-scratch-backed (borrowed) when the
///   requested length still fits the shared 64 KiB buffer, heap-backed otherwise.
/// - x2/rdx: actual bytes read (0 on EOF/error)
///
/// # Side effects
/// - Advances `_concat_off` by the actual bytes read, but only for scratch-backed results.
/// - Sets state-owned EOF on zero-byte reads and seekable short reads.
pub fn emit_fread(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fread_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread_raw");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stream, descriptor, and read-result spill slots
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save handle and requested length, then resolve the backend descriptor --
    emitter.instruction("str x0, [sp, #0]");                                    // save the opaque stream handle
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length
    // A FAILED read and a legitimate zero-byte read both end with length 0, so length alone
    // cannot tell PHP's `false` from its `""`. Slot 40 carries that distinction and leaves in
    // x0, beside the x1/x2 string pair.
    emitter.instruction("mov x9, #1");
    emitter.instruction("str x9, [sp, #40]");                                   // "this is a real result" — cleared only by an actual read failure
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor through StreamState
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the resolved backend descriptor
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is the backend below the wrapper range?
    emitter.instruction("b.lo __rt_fread_real_fd");                             // native descriptors continue to the syscall path
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("add x10, x9, x10");                                    // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp x0, x10");                                         // is the backend above the wrapper range?
    emitter.instruction("b.hs __rt_fread_real_fd");                             // non-wrapper synthetic backends stay on the native path
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the requested byte count for stream_read
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame before wrapper tail dispatch
    emitter.instruction("add sp, sp, #64");                                     // release native-read scratch storage
    emitter.instruction("b __rt_user_wrapper_fread");                           // wrapper backend tail-calls stream_read
    emitter.label("__rt_fread_real_fd");

    // -- reserve a destination sized for the whole requested read --
    // `__rt_stream_fd` clobbered x1, so the requested length comes back off the stack.
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the requested byte count
    emitter.instruction("cmp x1, #1");                                          // does the caller actually request at least one byte?
    emitter.instruction("b.lt __rt_fread_dest_scratch");                        // non-positive requests write nothing, so keep the current scratch tail
    emitter.instruction("mov x0, x1");                                          // request storage for the full requested read length
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the incoming bytes
    emitter.instruction("mov x12, x0");                                         // destination pointer for the read
    emitter.instruction("b __rt_fread_dest_ready");                             // the destination window is reserved
    emitter.label("__rt_fread_dest_scratch");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.label("__rt_fread_dest_ready");
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- hand back what a refused `stream_get_line()` held, before touching the descriptor --
    //
    // php keeps those bytes in the stream's own read buffer, which every read function shares: a
    // `stream_get_line()` that answered `false` is followed by an `fread()` that sees them.
    // Measured on `php -n` 8.5.6 over a non-blocking socket pair, "abc" with no newline is refused
    // by `stream_get_line()` and then read by `fread($h, 10)`. A stream that never hit that
    // refusal holds nothing, so this costs one call that returns 0 on its first load.
    emitter.instruction("ldr x0, [sp, #0]");                                    // the opaque stream handle
    emitter.instruction("ldr x1, [sp, #16]");                                   // the reserved destination
    emitter.instruction("ldr x2, [sp, #8]");                                    // at most the requested count
    emitter.instruction("bl __rt_stream_pending_take");                         // x0 = how many came back
    emitter.instruction("cbz x0, __rt_fread_no_pending");
    emitter.instruction("str x0, [sp, #24]");                                   // they are the whole result
    emitter.instruction("ldr x1, [sp, #16]");
    emitter.instruction("mov x2, x0");
    emitter.instruction("bl __rt_concat_publish");                              // claim the window they occupy
    emitter.instruction("b __rt_fread_done");
    emitter.label("__rt_fread_no_pending");

    // -- TLS dispatch: the session hangs off the StreamState, so it is keyed by
    //    the generation-checked handle rather than by a reusable descriptor. --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_tls_session");                          // x0 = attached session, zero when plain
    emitter.instruction("cbz x0, __rt_fread_do_syscall");                       // no TLS attached → fall through to read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // buf ptr: x12 is caller-saved and the call clobbered it
    emitter.instruction("ldr x2, [sp, #8]");                                    // len
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load elephc_tls_read entry pointer
    emitter.instruction("blr x9");                                              // x0 = bytes read (>=0) or -1
    emitter.instruction("cmp x0, #0");                                          // value-based check after the TLS call
    emitter.instruction("b.ge __rt_fread_read_ok");                             // continue when TLS read returned >= 0
    emitter.instruction("str xzr, [sp, #24]");                                  // TLS error: zero-length result
    emitter.instruction("str xzr, [sp, #40]");                                  // a failed TLS read is a failed read
    emitter.instruction("b __rt_fread_done");                                   // and does not exhaust the stream either

    emitter.label("__rt_fread_do_syscall");
    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #32]");                                   // fd for read syscall
    emitter.instruction("ldr x1, [sp, #16]");                                   // buffer pointer: the TLS probe clobbers caller-saved x12
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
    emitter.instruction("str xzr, [sp, #24]");                                  // failed reads carry no bytes
    emitter.instruction("str xzr, [sp, #40]");                                  // php answers false for a read that fails
    emitter.instruction("b __rt_fread_done");                                   // a FAILED read does not exhaust the stream: php keeps feof() false
    emitter.label("__rt_fread_read_ok");

    // -- publish the bytes actually read into the reserved destination --
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the reserved destination pointer
    emitter.instruction("mov x2, x0");                                          // pass the number of bytes actually read
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed reads

    // -- set EOF when the read returned fewer bytes than requested --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested byte count
    emitter.instruction("cmp x0, x9");                                          // did the backend satisfy the complete request?
    emitter.instruction("b.ge __rt_fread_done");                                // a full read does not prove EOF
    emitter.instruction("cbz x0, __rt_fread_mark_eof");                         // a zero-byte read is EOF for every blocking backend
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the backend descriptor for a seekability probe
    emitter.instruction("mov x1, #0");                                          // probe the current position without moving it
    emitter.instruction("mov x2, #1");                                          // select SEEK_CUR for the non-mutating probe
    emitter.syscall(199);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux reports a non-seekable descriptor as a negative result
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fread_mark_eof")); // only seekable short reads prove EOF
    emitter.instruction("b __rt_fread_done");                                   // sockets and pipes may legally return partial data
    emitter.label("__rt_fread_mark_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("mov x1, #1");                                          // publish the EOF state
    emitter.instruction("bl __rt_stream_eof_set");                              // update only this stream's stable state
    emitter.instruction("b __rt_fread_done");                                   // preserve a successful short-read result

    emitter.label("__rt_fread_would_block");
    emitter.instruction("str xzr, [sp, #24]");                                  // return an empty read without setting EOF for EAGAIN/EWOULDBLOCK

    // -- return pointer and length --
    emitter.label("__rt_fread_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return actual bytes read as length

    // Legacy per-descriptor slots still serve the filter families not yet moved
    // onto the chain (user filters, zlib/bzip2/iconv). A filter lives in exactly
    // one mechanism, so running both is correct for the duration of the migration.
    // -- apply attached read filters to the bytes just read (2-slot chain) --
    //    Slot 0 = _stream_read_filters[fd], slot 1 = _stream_read_filters[fd+256]
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the file descriptor
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    // -- slot 0 --
    emitter.instruction("ldrb w3, [x9, x0]");                                   // read filter id for slot 0
    emitter.instruction("cbz w3, __rt_fread_slot1");                            // skip slot 0 when empty
    emitter.instruction("cmp w3, #128");                                        // user-filter id range?
    emitter.instruction("b.lt __rt_fread_builtin_slot0");                       // built-in filter
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read)
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← transformed
    emitter.instruction("b __rt_fread_slot1");                                  // proceed to slot 1
    emitter.label("__rt_fread_builtin_slot0");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform in place
    // -- slot 1 --
    emitter.label("__rt_fread_slot1");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload fd
    emitter.instruction("add x10, x0, #256");                                   // fd+256 (slot 1 index)
    emitter.instruction("ldrb w3, [x9, x10]");                                  // read filter id for slot 1
    emitter.instruction("cbz w3, __rt_fread_ret");                              // skip slot 1 when empty
    emitter.instruction("cmp w3, #128");                                        // user-filter id range?
    emitter.instruction("b.lt __rt_fread_builtin_slot1");                       // built-in filter
    emitter.instruction("mov x3, #0");                                          // direction = 0 (read)
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← transformed
    emitter.instruction("b __rt_fread_ret");                                    // common epilogue
    emitter.label("__rt_fread_builtin_slot1");
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform in place


    // -- restore frame and return --
    emitter.label("__rt_fread_ret");
    // The flag rides out in x0, beside the x1/x2 string pair, exactly as
    // `__rt_stream_get_line` reports its own. The result pointer is deliberately NOT
    // nulled: x86_64 already returns a null pointer for an ordinary EOF, so the pointer
    // cannot mean "failed" on both arches, and every caller here tests the length first.
    emitter.instruction("ldr x0, [sp, #40]");                                   // x0 = 0 only when the read failed
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_fread` using libc `read()`.
fn emit_fread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread_raw");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fread() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor, length, and concat-buffer start pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned stream, descriptor, and read-result spill slots

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested byte count across the concat-buffer address computation and libc read() call
    // See the AArch64 half: a failed read and an empty one both end with length 0, so the
    // difference travels in its own slot and leaves in rcx.
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // "this is a real result" — cleared only by an actual read failure
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor through StreamState
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the resolved backend descriptor
    emitter.instruction("mov r9d, 0x40000000");                                 // materialize USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is the backend below the wrapper range?
    emitter.instruction("jb __rt_fread_real_fd_x86");                           // native descriptors continue to libc read
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("add r10, r9");                                         // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp rax, r10");                                        // is the backend above the wrapper range?
    emitter.instruction("jae __rt_fread_real_fd_x86");                          // non-wrapper synthetic backends stay on the native path
    emitter.instruction("mov rdi, rax");                                        // pass the synthetic backend descriptor to stream_read
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested byte count
    emitter.instruction("add rsp, 48");                                         // release native-read scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_user_wrapper_fread");                         // wrapper backend tail-calls stream_read
    emitter.label("__rt_fread_real_fd_x86");
    emitter.instruction("cmp rax, 0");                                          // did descriptor resolution produce a valid backend?
    emitter.instruction("jge __rt_fread_fd_ok_x86");                            // continue to the normal read path for non-negative descriptors
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer for an invalid stream
    emitter.instruction("xor edx, edx");                                        // return an empty string length for an invalid stream
    emitter.instruction("add rsp, 48");                                         // release native-read scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // skip the read path for invalid stream handles

    emitter.label("__rt_fread_fd_ok_x86");
    // -- reserve a destination sized for the whole requested read --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the requested byte count after the descriptor resolution
    emitter.instruction("cmp rsi, 1");                                          // does the caller actually request at least one byte?
    emitter.instruction("jl __rt_fread_dest_scratch_x86");                      // non-positive requests write nothing, so keep the current scratch tail
    emitter.instruction("mov rax, rsi");                                        // request storage for the full requested read length (reserve reads rax)
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the incoming bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the reserved destination pointer for the final elephc string result
    emitter.instruction("jmp __rt_fread_dest_ready_x86");                       // the destination window is reserved
    emitter.label("__rt_fread_dest_scratch_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // load the current concat-buffer absolute offset before appending the fread() result
    abi::emit_symbol_address(emitter, "r11", "_concat_buf");                    // materialize the concat-buffer base address once for the x86_64 fread() helper
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the start pointer for the bytes that libc read() will append
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the concat-buffer start pointer for the final elephc string result
    emitter.label("__rt_fread_dest_ready_x86");

    // -- hand back what a refused `stream_get_line()` held, before touching the descriptor --
    // See the AArch64 counterpart: php keeps those bytes in the stream's shared read buffer.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the opaque stream handle
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the reserved destination
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // at most the requested count
    emitter.instruction("call __rt_stream_pending_take");                       // rax = how many came back
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fread_no_pending_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // they are the whole result
    emitter.instruction("mov rdx, rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_concat_publish");                            // claim the window they occupy
    emitter.instruction("jmp __rt_fread_publish_x86");
    emitter.label("__rt_fread_no_pending_x86");

    // -- TLS dispatch: the session hangs off the StreamState, so it is keyed by
    //    the generation-checked handle rather than by a reusable descriptor. --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_tls_session");                        // rax = attached session, zero when plain
    emitter.instruction("test rax, rax");                                       // is a TLS session attached?
    emitter.instruction("jz __rt_fread_do_syscall_x86");                        // no TLS attached → use libc read
    emitter.instruction("mov rdi, rax");                                        // handle as first arg
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // buf ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // len
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_read_fn", 0);      // prepare SysV call argument
    emitter.instruction("call r9");                                             // rax = bytes read (>=0) or -1
    emitter.instruction("cmp rax, 0");                                          // did the TLS bridge return bytes?
    emitter.instruction("jl __rt_fread_tls_failed_x86");                        // strictly negative is an error, not an exhausted stream
    emitter.instruction("jz __rt_fread_eof_x86");                               // a zero-byte TLS read is an ordinary EOF
    emitter.instruction("jmp __rt_fread_read_ok_x86");                          // publish the successful TLS read
    emitter.label("__rt_fread_tls_failed_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // a failed TLS read is a failed read
    emitter.instruction("jmp __rt_fread_failed_x86");                           // and does not exhaust the stream either
    emitter.label("__rt_fread_do_syscall_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc read() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the concat-buffer write pointer as the second libc read() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the requested byte count as the third libc read() argument
    emitter.instruction("call read");                                           // read the requested bytes into the concat-buffer append window through libc read()
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as bytes, EOF, or failure
    emitter.instruction("jg __rt_fread_read_ok_x86");                           // positive byte count: publish the successful read
    emitter.instruction("jl __rt_fread_read_failed_x86");                       // negative result: inspect errno before treating it as EOF
    emitter.instruction("jmp __rt_fread_eof_x86");                              // zero-byte read means real EOF

    emitter.label("__rt_fread_read_ok_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the actual byte count across EOF publication
    emitter.instruction("mov rdx, rax");                                        // pass the number of bytes actually read
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // pass the reserved destination pointer
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed reads
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the byte count publish left in rdx for the EOF classification below
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // did the backend satisfy the complete request?
    emitter.instruction("jge __rt_fread_publish_x86");                          // a full read does not prove EOF
    emitter.instruction("test rax, rax");                                       // was this a universal zero-byte EOF read?
    emitter.instruction("jz __rt_fread_mark_eof_x86");                          // zero bytes mark every blocking backend exhausted
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the backend descriptor for a seekability probe
    emitter.instruction("xor esi, esi");                                        // probe the current position without moving it
    emitter.instruction("mov edx, 1");                                          // select SEEK_CUR for the non-mutating probe
    emitter.instruction("call lseek");                                          // seekable short reads prove the regular stream was exhausted
    emitter.instruction("test rax, rax");                                       // did the position probe fail?
    emitter.instruction("js __rt_fread_publish_x86");                           // sockets and pipes may legally return partial data
    emitter.label("__rt_fread_mark_eof_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov esi, 1");                                          // publish the EOF state
    emitter.instruction("call __rt_stream_eof_set");                            // update only this stream's stable state
    emitter.label("__rt_fread_publish_x86");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the successful byte count in the string-length register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer in the x86_64 elephc string-pointer result register
    // Legacy per-descriptor slots still serve the filter families not yet moved
    // onto the chain (user filters, zlib/bzip2/iconv). A filter lives in exactly
    // one mechanism, so running both is correct for the duration of the migration.
    // -- apply attached read filters (2-slot chain) --
    //    Slot 0 = _stream_read_filters[fd], slot 1 = _stream_read_filters[fd+256]
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the file descriptor
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // materialize the read-filter table base
    // -- slot 0 --
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // read filter id for slot 0
    emitter.instruction("test rcx, rcx");                                       // is slot 0 empty?
    emitter.instruction("jz __rt_fread_slot1_x86");                             // skip slot 0 when empty
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range?
    emitter.instruction("jl __rt_fread_builtin_slot0_x86");                     // built-in filter
    emitter.instruction("mov rdi, r10");                                        // fd
    emitter.instruction("mov rsi, rax");                                        // buf ptr
    // rdx already holds the byte count
    emitter.instruction("xor ecx, ecx");                                        // direction = 0 (read)
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← transformed
    emitter.instruction("jmp __rt_fread_slot1_x86");                            // proceed to slot 1
    emitter.label("__rt_fread_builtin_slot0_x86");
    emitter.instruction("call __rt_apply_stream_filter");                       // transform in place
    // -- slot 1 --
    emitter.label("__rt_fread_slot1_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload fd
    abi::emit_symbol_address(emitter, "r11", "_stream_read_filters");           // materialize the read-filter table base
    emitter.instruction("lea rcx, [r10 + 256]");                                // fd+256 (slot 1 index)
    emitter.instruction("movzx ecx, BYTE PTR [r11 + rcx]");                     // read filter id for slot 1
    emitter.instruction("test rcx, rcx");                                       // is slot 1 empty?
    emitter.instruction("jz __rt_fread_ret_x86");                               // skip slot 1 when empty
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range?
    emitter.instruction("jl __rt_fread_builtin_slot1_x86");                     // built-in filter
    emitter.instruction("mov rdi, r10");                                        // fd
    emitter.instruction("mov rsi, rax");                                        // buf ptr (post slot-0)
    // rdx already holds the byte count (post slot-0)
    emitter.instruction("xor ecx, ecx");                                        // direction = 0 (read)
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← transformed
    emitter.instruction("jmp __rt_fread_ret_x86");                              // common epilogue
    emitter.label("__rt_fread_builtin_slot1_x86");
    emitter.instruction("call __rt_apply_stream_filter");                       // transform in place
    emitter.label("__rt_fread_ret_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // rcx = 0 only when the read failed
    emitter.instruction("add rsp, 48");                                         // release the fread() spill slots before returning the successful string slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the successful fread() path
    emitter.instruction("ret");                                                 // return the borrowed concat-buffer string slice to the caller

    emitter.label("__rt_fread_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_fread_would_block_x86");                       // transient nonblocking miss returns empty without EOF
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // php answers false for a read that fails
    emitter.instruction("jmp __rt_fread_failed_x86");                           // a FAILED read does not exhaust the stream

    emitter.label("__rt_fread_would_block_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer for an empty transient read
    emitter.instruction("xor edx, edx");                                        // return a zero-length read result without setting EOF
    emitter.instruction("mov ecx, 1");                                          // a would-block is an empty READ, not a failure
    emitter.instruction("add rsp, 48");                                         // release the fread() spill slots before returning the empty string
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the would-block fread() path
    emitter.instruction("ret");                                                 // return the empty non-EOF read result

    // A failed read: the same empty result as EOF, but the stream is NOT marked exhausted.
    emitter.label("__rt_fread_failed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the reserved destination, so the pointer stays valid
    emitter.instruction("xor edx, edx");                                        // no bytes were read
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // rcx = 0: the caller boxes PHP false
    emitter.instruction("add rsp, 48");                                         // release the fread() spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failed-read result

    emitter.label("__rt_fread_eof_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("mov esi, 1");                                          // publish the EOF state
    emitter.instruction("call __rt_stream_eof_set");                            // mark this stream exhausted after the zero-byte or failed read
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when libc read() reports EOF or failure
    emitter.instruction("xor edx, edx");                                        // return an empty string length when libc read() reports EOF or failure
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // EOF and failure share this path; the slot tells them apart
    emitter.instruction("add rsp, 48");                                         // release the fread() spill slots before returning the empty-string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the EOF/error fread() path
    emitter.instruction("ret");                                                 // return the empty string result for the exhausted or failed stream read
}
