//! Purpose:
//! Emits the `__rt_fgets`, `__rt_fgets_fd_ok` runtime helper assembly for fgets.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.
//! - `fgets()` has no caller-supplied length bound, so the line accumulates into a
//!   `__rt_concat_reserve` window that doubles through `__rt_concat_grow` whenever it fills.
//!   A line longer than the 64 KiB concat scratch therefore produces owned heap storage
//!   instead of running past `_concat_buf` into the adjacent BSS globals.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;

/// Initial reserved line capacity, in bytes. Long enough that ordinary text lines never grow,
/// small enough that a `while (fgets($f))` loop still stays inside the shared concat scratch.
const INITIAL_LINE_CAPACITY: usize = 4096;

/// Reads one line from a file descriptor into the concat buffer.
/// dispatches to `emit_fgets_linux_x86_64` on x86_64; falls through to ARM64
/// path otherwise.
///
/// ABI contract:
///   - input:  x0 = file descriptor (non-negative for valid fds, negative for
///             errors such as failed fopen)
///   - output: x1 = pointer to line start (concat scratch or owned heap storage),
///             x2 = line length (includes trailing `\n` if one was present before EOF)
///   - side effect: sets `_eof_flags[fd] = 1` when the stream is exhausted
///   - the reservation is published with the final line length, so the shared concat
///     offset only moves for scratch-backed lines; a line may be partial if EOF or a
///     read error interrupts the stream
pub fn emit_fgets(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fgets_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    // -- check for invalid fd (negative = fopen failed) --
    emitter.instruction("cmp x0, #0");                                          // check if fd is negative
    emitter.instruction("b.ge __rt_fgets_fd_ok");                               // if fd >= 0, proceed normally
    emitter.instruction("mov x1, #0");                                          // return empty string: null pointer
    emitter.instruction("mov x2, #0");                                          // return empty string: zero length
    emitter.instruction("ret");                                                 // return immediately to caller

    // -- set up stack frame --
    emitter.label("__rt_fgets_fd_ok");
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save fd and reserve an initial line window --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor on stack
    emitter.instruction(&format!("mov x9, #{}", INITIAL_LINE_CAPACITY));        // start from the initial line capacity
    emitter.instruction("str x9, [sp, #24]");                                   // save the current line capacity
    emitter.instruction("mov x0, x9");                                          // request the initial capacity from the reservation front end
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the line
    emitter.instruction("str x0, [sp, #16]");                                   // save start pointer for return value
    emitter.instruction("mov x1, x0");                                          // publish the reservation start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // publish the full reserved capacity
    emitter.instruction("bl __rt_concat_publish");                              // claim the whole window so nested reads append after it
    emitter.instruction("str xzr, [sp, #8]");                                   // the line starts empty

    // -- user-wrapper fd: read the line through stream_read instead of read() --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("mov w9, #0x4000");                                     // high half of USER_WRAPPER_FD_BASE
    emitter.instruction("lsl w9, w9, #16");                                     // form 0x40000000 in w9
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.ge __rt_fgets_wrapper_entry");                       // wrappers read through the post-read-EOF stream_read loop below

    // -- read loop: one byte at a time until \n or EOF --
    emitter.label("__rt_fgets_loop");

    // -- make room for one more byte before reading it --
    emitter.instruction("ldr x9, [sp, #8]");                                    // current line length
    emitter.instruction("add x9, x9, #1");                                      // capacity needed once the next byte lands
    emitter.instruction("ldr x10, [sp, #24]");                                  // current line capacity
    emitter.instruction("cmp x9, x10");                                         // does the next byte still fit the reservation?
    emitter.instruction("b.ls __rt_fgets_have_room");                           // no growth needed for this iteration
    emitter.instruction("lsl x10, x10, #1");                                    // double the line capacity
    emitter.instruction("str x10, [sp, #24]");                                  // save the grown line capacity
    emitter.instruction("ldr x1, [sp, #16]");                                   // old line buffer pointer
    emitter.instruction("mov x2, #0");                                          // release the whole claimed window
    emitter.instruction("bl __rt_concat_publish");                              // hand the old scratch window back before moving to heap storage
    emitter.instruction("ldr x0, [sp, #16]");                                   // old line buffer pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // bytes accumulated so far must survive the move
    emitter.instruction("ldr x2, [sp, #24]");                                   // grown line capacity
    emitter.instruction("bl __rt_concat_grow");                                 // move the accumulated line into a larger owned buffer
    emitter.instruction("str x0, [sp, #16]");                                   // save the grown line buffer pointer
    emitter.label("__rt_fgets_have_room");
    emitter.instruction("ldr x1, [sp, #16]");                                   // line buffer base pointer
    emitter.instruction("ldr x10, [sp, #8]");                                   // current line length
    emitter.instruction("add x1, x1, x10");                                     // buf pointer for read syscall

    // -- read 1 byte via syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for read syscall
    emitter.instruction("mov x2, #1");                                          // read exactly 1 byte
    emitter.syscall(3);

    // -- check if read failed or returned 0 (EOF) --
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: check return value
        emitter.instruction("b.eq __rt_fgets_eof");                             // zero-byte read means EOF
        emitter.instruction("b.lt __rt_fgets_read_failed");                     // negative result: inspect errno before setting EOF
    } else {
        emitter.instruction("b.cs __rt_fgets_read_failed");                     // macOS: if carry set, inspect errno before setting EOF
        emitter.instruction("cbz x0, __rt_fgets_eof");                          // if 0 bytes read, we hit EOF
    }

    // -- count the byte just appended to the line --
    emitter.instruction("ldr x11, [sp, #16]");                                  // line buffer base pointer
    emitter.instruction("ldr x13, [sp, #8]");                                   // offset of byte just read
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the byte we just read
    emitter.instruction("add x13, x13, #1");                                    // advance the line length by 1 byte
    emitter.instruction("str x13, [sp, #8]");                                   // store the updated line length

    // -- check if the byte we just read is \n --
    emitter.instruction("cmp w14, #0x0A");                                      // compare with newline character
    emitter.instruction("b.eq __rt_fgets_done");                                // if newline, line is complete
    emitter.instruction("b __rt_fgets_loop");                                   // otherwise continue reading

    // -- user-wrapper line read: stream_read one byte at a time until empty.
    //    Each read performs php-src's lenient post-read stream_eof callback. Bytes are
    //    appended to _user_wrapper_drain_buf — NOT _concat_buf, because each
    //    __rt_fread result may itself occupy _concat_buf and clobber the line.
    //    [sp,#8] tracks the accumulated line length; the line is returned
    //    directly (this path does not reuse the _concat_buf-based done label).
    emitter.label("__rt_fgets_wrapper_entry");
    emitter.instruction("ldr x1, [sp, #16]");                                   // the wrapper path accumulates elsewhere, so give the reservation back
    emitter.instruction("mov x2, #0");                                          // release the whole claimed window
    emitter.instruction("bl __rt_concat_publish");                              // hand the unused line window back to the shared scratch buffer
    emitter.instruction("str xzr, [sp, #8]");                                   // line length = 0
    emitter.label("__rt_fgets_wrapper_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the wrapper fd
    emitter.instruction("mov x1, #1");                                          // read exactly one byte
    emitter.instruction("bl __rt_fread");                                       // x1 = chunk ptr, x2 = len
    emitter.instruction("cbz x2, __rt_fgets_wrapper_done");                     // defensive: empty read also ends the line
    emitter.instruction("ldrb w13, [x1]");                                      // load the read byte
    emitter.instruction("ldr x10, [sp, #8]");                                   // current line length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_user_wrapper_drain_buf");
    emitter.instruction("strb w13, [x12, x10]");                                // append the byte to the line buffer
    emitter.instruction("add x10, x10, #1");                                    // advance the line length
    emitter.instruction("str x10, [sp, #8]");                                   // store the updated line length
    emitter.instruction("strb w13, [sp, #32]");                                 // remember the byte across the chunk-release calls
    emitter.instruction("mov x2, #0");                                          // release the whole chunk window
    emitter.instruction("bl __rt_concat_publish");                              // hand this chunk's scratch window back before the next read
    emitter.instruction("mov x0, x1");                                          // chunk ptr for release
    emitter.instruction("bl __rt_decref_any");                                  // release the chunk before continuing or finishing the line
    emitter.instruction("ldrb w13, [sp, #32]");                                 // reload the byte the wrapper produced
    emitter.instruction("cmp w13, #0x0A");                                      // is the byte a newline?
    emitter.instruction("b.eq __rt_fgets_wrapper_done");                        // newline: finish the line
    emitter.instruction("b __rt_fgets_wrapper_loop");                           // read the next byte
    emitter.label("__rt_fgets_wrapper_done");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_user_wrapper_drain_buf"); // line pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // line length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return the wrapper line (ptr/len)

    // -- nonblocking read miss: return accumulated bytes without EOF --
    emitter.label("__rt_fgets_read_failed");
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction(&format!("cmn x0, #{}", emitter.platform.would_block_errno())); // Linux: compare read result with -EAGAIN/-EWOULDBLOCK
    } else {
        emitter.instruction(&format!("cmp x0, #{}", emitter.platform.would_block_errno())); // macOS: compare errno with EAGAIN/EWOULDBLOCK
    }
    emitter.instruction("b.eq __rt_fgets_done");                                // would-block returns the partial line, not EOF

    // -- EOF reached: set eof flag for this fd --
    emitter.label("__rt_fgets_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // eof marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1

    // -- return result string --
    emitter.label("__rt_fgets_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the accumulated line length
    emitter.instruction("bl __rt_concat_publish");                              // shrink the claimed window down to the bytes actually read

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64-specific fgets: mirrors the ARM64 path using x86_64 System V ABI.
/// Input:  rdi = file descriptor
/// Output: rax = line start pointer (concat scratch or owned heap storage), rdx = line length
/// Side effect: sets _eof_flags[fd] = 1 when stream is exhausted
fn emit_fgets_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    emitter.instruction("cmp rdi, 0");                                          // does fgets() have a valid non-negative file descriptor to read from?
    emitter.instruction("jge __rt_fgets_fd_ok_x86");                            // continue to the normal line-read path when the file descriptor is valid
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer immediately when fopen() failed
    emitter.instruction("xor edx, edx");                                        // return an empty string length immediately when fopen() failed
    emitter.instruction("ret");                                                 // skip the line-read loop entirely for invalid file descriptors

    emitter.label("__rt_fgets_fd_ok_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fgets() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor and concat-buffer start metadata
    emitter.instruction("sub rsp, 48");                                         // reserve aligned stack space for the stream read loop temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the file descriptor across the repeated libc read() calls in the fgets() loop
    emitter.instruction(&format!("mov QWORD PTR [rbp - 40], {}", INITIAL_LINE_CAPACITY)); // start from the initial line capacity
    emitter.instruction(&format!("mov rax, {}", INITIAL_LINE_CAPACITY));        // request the initial capacity from the reservation front end
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the line
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the line start pointer for the final elephc string result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // publish the full reserved capacity
    emitter.instruction("call __rt_concat_publish");                            // claim the whole window so nested reads append after it
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // the line starts empty

    // -- user-wrapper fd: read the line through stream_read instead of read() --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_fgets_wrapper_entry_x86");                    // wrappers read through the post-read-EOF stream_read loop below

    emitter.label("__rt_fgets_loop_x86");

    // -- make room for one more byte before reading it --
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // current line length
    emitter.instruction("add r8, 1");                                           // capacity needed once the next byte lands
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // current line capacity
    emitter.instruction("cmp r8, r9");                                          // does the next byte still fit the reservation?
    emitter.instruction("jbe __rt_fgets_have_room_x86");                        // no growth needed for this iteration
    emitter.instruction("add r9, r9");                                          // double the line capacity
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the grown line capacity
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // old line buffer pointer
    emitter.instruction("xor edx, edx");                                        // release the whole claimed window
    emitter.instruction("call __rt_concat_publish");                            // hand the old scratch window back before moving to heap storage
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // old line buffer pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // bytes accumulated so far must survive the move
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // grown line capacity
    emitter.instruction("call __rt_concat_grow");                               // move the accumulated line into a larger owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the grown line buffer pointer
    emitter.label("__rt_fgets_have_room_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // line buffer base pointer
    emitter.instruction("add rsi, QWORD PTR [rbp - 16]");                       // compute the address where libc read() should append the next byte
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the tracked file descriptor as the first libc read() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one byte so fgets() can stop on the first newline
    emitter.instruction("call read");                                           // read one byte from the stream through libc read() into the concat buffer
    emitter.instruction("cmp rax, 0");                                          // classify libc read() as a byte, EOF, or failure
    emitter.instruction("jg __rt_fgets_read_ok_x86");                           // positive byte count: publish the appended byte
    emitter.instruction("jl __rt_fgets_read_failed_x86");                       // negative result: inspect errno before setting EOF
    emitter.instruction("jmp __rt_fgets_eof_x86");                              // zero-byte read means real EOF

    emitter.label("__rt_fgets_read_ok_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // line buffer base pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // offset of the byte just read
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10]");                     // load the byte that was just appended to the line
    emitter.instruction("add r10, 1");                                          // advance the line length by the one byte that libc read() appended
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // store the updated line length
    emitter.instruction("cmp cl, 0x0A");                                        // did the newly appended byte terminate the line with a newline?
    emitter.instruction("jne __rt_fgets_loop_x86");                             // keep reading until fgets() hits newline, EOF, or read failure

    emitter.label("__rt_fgets_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the reserved start pointer for the fgets() line
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the accumulated line length in the x86_64 elephc string-length result register
    emitter.instruction("call __rt_concat_publish");                            // shrink the claimed window down to the bytes actually read
    emitter.instruction("add rsp, 48");                                         // release the fgets() spill slots before returning the line slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 fgets() helper completes
    emitter.instruction("ret");                                                 // return the fgets() line slice to the caller

    // -- user-wrapper line read: stream_read one byte at a time until empty.
    //    Each read performs php-src's lenient post-read stream_eof callback. Bytes are
    //    appended to _user_wrapper_drain_buf — NOT _concat_buf, because each
    //    __rt_fread result may itself occupy _concat_buf and clobber the line.
    //    [rbp-16] tracks the accumulated line length; the line is returned
    //    directly (this path does not reuse the _concat_buf-based done label).
    emitter.label("__rt_fgets_wrapper_entry_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the wrapper path accumulates elsewhere, so give the reservation back
    emitter.instruction("xor edx, edx");                                        // release the whole claimed window
    emitter.instruction("call __rt_concat_publish");                            // hand the unused line window back to the shared scratch buffer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // line length = 0
    emitter.label("__rt_fgets_wrapper_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the wrapper fd
    emitter.instruction("mov rsi, 1");                                          // read exactly one byte
    emitter.instruction("call __rt_fread");                                     // rax = chunk ptr, rdx = len
    emitter.instruction("test rdx, rdx");                                       // zero-length read?
    emitter.instruction("jz __rt_fgets_wrapper_done_x86");                      // defensive: empty read also ends the line
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the chunk ptr across the per-chunk release
    emitter.instruction("movzx ecx, BYTE PTR [rax]");                           // load the read byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // current line length
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_drain_buf");        // line buffer base
    emitter.instruction("mov BYTE PTR [r11 + r10], cl");                        // append the byte to the line buffer
    emitter.instruction("add r10, 1");                                          // advance the line length
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // store the updated line length
    emitter.instruction("mov BYTE PTR [rbp - 48], cl");                         // remember the byte across the chunk-release calls
    emitter.instruction("xor edx, edx");                                        // release the whole chunk window
    emitter.instruction("call __rt_concat_publish");                            // hand this chunk's scratch window back before the next read
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // chunk ptr for release
    emitter.instruction("call __rt_decref_any");                                // release the chunk before continuing or finishing the line
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 48]");                      // reload the byte the wrapper produced
    emitter.instruction("cmp cl, 0x0A");                                        // is the byte a newline?
    emitter.instruction("je __rt_fgets_wrapper_done_x86");                      // newline: finish the line
    emitter.instruction("jmp __rt_fgets_wrapper_loop_x86");                     // read the next byte
    emitter.label("__rt_fgets_wrapper_done_x86");
    abi::emit_symbol_address(emitter, "rax", "_user_wrapper_drain_buf");        // line pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // line length
    emitter.instruction("add rsp, 48");                                         // release the fgets() spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the wrapper line (ptr/len)

    emitter.label("__rt_fgets_read_failed_x86");
    emitter.instruction("call __errno_location");                               // fetch errno after libc read() failed
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load the thread-local errno value
    emitter.instruction("cmp r10d, 11");                                        // is this EAGAIN/EWOULDBLOCK from a nonblocking fd?
    emitter.instruction("je __rt_fgets_done_x86");                              // would-block returns the partial line without setting EOF

    emitter.label("__rt_fgets_eof_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor so the eof-flag table can mark this stream as exhausted
    abi::emit_symbol_address(emitter, "r11", "_eof_flags");                     // materialize the eof-flag table base address for the current stream descriptor
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // mark the current file descriptor as EOF-reached after the zero-byte or failed read
    emitter.instruction("jmp __rt_fgets_done_x86");                             // return the possibly empty borrowed slice accumulated before EOF or read failure
}
