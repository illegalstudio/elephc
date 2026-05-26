//! Purpose:
//! Emits stream-extension runtime helpers (`__rt_fgetc`, `__rt_readfile`,
//! `__rt_fpassthru`, `__rt_flock`, `__rt_tmpfile`).
//! Bridges PHP stream-side builtins to libc/syscalls for ARM64 (Darwin/Linux)
//! and Linux x86_64.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` and
//!   `crate::codegen::runtime::x86_minimal::emit_runtime_linux_x86_64_minimal()`.
//!
//! Key details:
//! - `__rt_fgetc` tail-calls `__rt_fread` with length = 1.
//! - `__rt_readfile`/`__rt_fpassthru` use a stack buffer and `read`+`write`
//!   loops; read paths must check Darwin's carry-flag error signaling before
//!   comparing byte counts.
//! - `__rt_flock` translates the PHP `LOCK_UN` value (3) to the POSIX value (8)
//!   while preserving the `LOCK_NB` flag bit.
//! - `__rt_tmpfile` returns the raw fd in x0/rax (-1 on failure); the codegen
//!   wrapper boxes it as resource/false via `__rt_mixed_from_value`.

use crate::codegen::{
    abi,
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Emits stream-extension runtime helpers for the current target.
///
/// Dispatches to `emit_streams_ext_linux_x86_64` when targeting x86_64 Linux;
/// otherwise emits ARM64 helpers for all other targets (Darwin, Linux ARM64).
///
/// ARM64 helpers emitted: `__rt_fgetc`, `__rt_readfile`, `__rt_fpassthru`, `__rt_flock`, `__rt_tmpfile`.
pub fn emit_streams_ext(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_streams_ext_linux_x86_64(emitter);
        return;
    }

    // ================================================================
    // __rt_fgetc: read one byte from an fd.
    // Input:  x0 = fd
    // Output: x1/x2 = result string (length 0 on EOF, length 1 otherwise)
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: fgetc ---");
    emitter.label_global("__rt_fgetc");
    emitter.instruction("mov x1, #1");                                          // read at most one byte
    emitter.instruction("b __rt_fread");                                        // tail-call into fread; the return values land in x1/x2 directly

    // ================================================================
    // __rt_readfile: open path, copy contents to stdout, return bytes
    // copied (-2 on open failure, -1 on read failure).
    // Input:  x1/x2 = path
    // Output: x0 = total bytes written
    // Frame layout (saved frame regs at offset 0 to keep stp/ldp imms
    // within range):
    //   sp+ 0  : x29 / x30
    //   sp+16  : fd
    //   sp+24  : total bytes copied
    //   sp+32  : 1024-byte read buffer
    // ================================================================
    let buf_size = 1024usize;
    let buf_off = 32usize;
    let frame_size = ((buf_off + buf_size) + 15) & !15;
    let save_off = 0usize;

    let fd_off = 16usize;
    let total_off = 24usize;

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: readfile ---");
    emitter.label_global("__rt_readfile");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate frame + read buffer
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address (low offset for imm range)
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.instruction(&format!("str xzr, [sp, #{}]", total_off));             // total bytes copied = 0

    // -- open(path, O_RDONLY) --
    emitter.instruction("bl __rt_cstr");                                        // path → null-terminated C string in x0
    emitter.instruction("mov x1, #0");                                          // O_RDONLY
    emitter.instruction("mov x2, #0");                                          // mode (unused for O_RDONLY)
    emitter.syscall(5);                                                         // open(path, flags, mode)
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: explicit compare for error branch
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_readfile_open_ok")); // platform-aware success branch (Darwin: b.cc / Linux: b.ge)
    emitter.instruction("b __rt_readfile_fail");                                // open failed → return failure sentinel
    emitter.label("__rt_readfile_open_ok");
    emitter.instruction(&format!("str x0, [sp, #{}]", fd_off));                 // save fd

    // -- loop: read(fd, buf, N); if 0 done; write(1, buf, n); accumulate --
    emitter.label("__rt_readfile_loop");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // buffer pointer
    emitter.instruction(&format!("mov x2, #{}", buf_size));                     // requested chunk size
    emitter.syscall(3);                                                         // read(fd, buf, count)
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: negative read result means failure
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_readfile_read_ok")); // continue only when read succeeded
    emitter.instruction("b __rt_readfile_read_error");                          // read failed → close fd and return -1
    emitter.label("__rt_readfile_read_ok");
    emitter.instruction("cmp x0, #0");                                          // bytes read?
    emitter.instruction("b.eq __rt_readfile_done");                             // EOF → stop
    emitter.instruction("mov x9, x0");                                          // preserve byte count for write
    emitter.instruction(&format!("ldr x10, [sp, #{}]", total_off));             // current total
    emitter.instruction("add x10, x10, x9");                                    // accumulate total
    emitter.instruction(&format!("str x10, [sp, #{}]", total_off));             // persist updated total
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // buffer pointer
    emitter.instruction("mov x2, x9");                                          // length to write
    emitter.syscall(4);                                                         // write(1, buf, n)
    emitter.instruction("b __rt_readfile_loop");                                // continue copying

    emitter.label("__rt_readfile_done");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.syscall(6);                                                         // close(fd)
    emitter.instruction(&format!("ldr x0, [sp, #{}]", total_off));              // total bytes copied
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return total bytes

    emitter.label("__rt_readfile_read_error");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd before returning a read-error result
    emitter.syscall(6);                                                         // close(fd) after the failed read
    emitter.instruction("mov x0, #-1");                                         // read failure sentinel, matching PHP's -1 byte count
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address (read-error path)
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame (read-error path)
    emitter.instruction("ret");                                                 // return read failure sentinel

    emitter.label("__rt_readfile_fail");
    emitter.instruction("mov x0, #-2");                                         // open failure sentinel for PHP false
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address (failure path)
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame (failure path)
    emitter.instruction("ret");                                                 // return failure sentinel

    // ================================================================
    // __rt_fpassthru: copy remaining contents of an open fd to stdout.
    // Input:  x0 = fd
    // Output: x0 = total bytes copied
    // Frame: same as __rt_readfile (1 KiB buffer + total counter slot)
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: fpassthru ---");
    emitter.label_global("__rt_fpassthru");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate frame + read buffer
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_off));         // save frame pointer and return address (low offset for imm range)
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.instruction(&format!("str x0, [sp, #{}]", fd_off));                 // save fd
    emitter.instruction(&format!("str xzr, [sp, #{}]", total_off));             // total bytes = 0

    emitter.label("__rt_fpassthru_loop");
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // buffer pointer
    emitter.instruction(&format!("mov x2, #{}", buf_size));                     // chunk size
    emitter.syscall(3);                                                         // read(fd, buf, count)
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: negative read result means failure
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fpassthru_read_ok")); // continue only when read succeeded
    emitter.instruction("b __rt_fpassthru_read_error");                         // read failed → return -1
    emitter.label("__rt_fpassthru_read_ok");
    emitter.instruction("cmp x0, #0");                                          // bytes read?
    emitter.instruction("b.eq __rt_fpassthru_done");                            // EOF → stop
    emitter.instruction("mov x9, x0");                                          // preserve byte count
    emitter.instruction(&format!("ldr x10, [sp, #{}]", total_off));             // current total
    emitter.instruction("add x10, x10, x9");                                    // accumulate total
    emitter.instruction(&format!("str x10, [sp, #{}]", total_off));             // persist total
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // buffer pointer
    emitter.instruction("mov x2, x9");                                          // length
    emitter.syscall(4);                                                         // write(1, buf, n)
    emitter.instruction("b __rt_fpassthru_loop");                               // continue

    emitter.label("__rt_fpassthru_done");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", fd_off));                 // reload fd so feof() observes that passthru reached EOF
    abi::emit_symbol_address(emitter, "x10", "_eof_flags");
    emitter.instruction("mov w11, #1");                                         // eof marker value for fpassthru completion
    emitter.instruction("strb w11, [x10, x9]");                                 // set _eof_flags[fd] after consuming the stream
    emitter.instruction(&format!("ldr x0, [sp, #{}]", total_off));              // total bytes copied
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame
    emitter.instruction("ret");                                                 // return total bytes

    emitter.label("__rt_fpassthru_read_error");
    emitter.instruction(&format!("ldr x9, [sp, #{}]", fd_off));                 // reload fd so feof() observes the exhausted error state
    abi::emit_symbol_address(emitter, "x10", "_eof_flags");
    emitter.instruction("mov w11, #1");                                         // eof marker value after fpassthru read failure
    emitter.instruction("strb w11, [x10, x9]");                                 // set _eof_flags[fd] after a failed read
    emitter.instruction("mov x0, #-1");                                         // read failure sentinel, matching PHP's -1 byte count
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_off));         // restore frame pointer and return address (read-error path)
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate frame (read-error path)
    emitter.instruction("ret");                                                 // return read failure sentinel

    // ================================================================
    // __rt_flock: libc flock(fd, op).
    // Input:  x0 = fd, x1 = lock op using the PHP numbering
    //         (LOCK_SH=1, LOCK_EX=2, LOCK_UN=3, LOCK_NB=4).
    // Output: x0 = 1 on success, 0 on failure
    //
    // Translates the PHP LOCK_UN value (3) to the POSIX flock value (8)
    // while preserving the LOCK_NB flag bit.
    // ================================================================
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: flock ---");
    emitter.label_global("__rt_flock");
    emitter.instruction("sub sp, sp, #16");                                     // allocate minimal frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer
    emitter.instruction("and x9, x1, #4");                                      // x9 = LOCK_NB bit
    emitter.instruction("and x10, x1, #3");                                     // x10 = base op (1, 2, or 3)
    emitter.instruction("cmp x10, #3");                                         // is base op LOCK_UN (PHP value 3)?
    emitter.instruction("b.ne __rt_flock_done_translate");                      // not LOCK_UN: keep base value as-is
    emitter.instruction("mov x10, #8");                                         // POSIX LOCK_UN = 8
    emitter.label("__rt_flock_done_translate");
    emitter.instruction("orr x1, x10, x9");                                     // recombine LOCK_NB flag with translated base
    emitter.bl_c("flock");                                                      // libc flock(fd, op)
    emitter.instruction("cmp w0, #0");                                          // did libc flock() succeed?
    emitter.instruction("b.ne __rt_flock_fail");                                // failed lock attempt: inspect errno for would-block state
    emitter.instruction("mov x1, #0");                                          // would_block output = false on success
    emitter.instruction("mov x0, #1");                                          // flock() returns true on success
    emitter.instruction("b __rt_flock_return");                                 // skip errno inspection after successful flock()
    emitter.label("__rt_flock_fail");
    let errno_func = match emitter.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
    };
    let would_block_errno = match emitter.platform {
        Platform::MacOS => 35,
        Platform::Linux => 11,
    };
    emitter.bl_c(errno_func);                                                    // fetch thread-local errno storage after flock() failure
    emitter.instruction("ldr w9, [x0]");                                        // load errno value set by libc flock()
    emitter.instruction(&format!("cmp w9, #{}", would_block_errno));            // compare errno with EWOULDBLOCK/EAGAIN for this platform
    emitter.instruction("cset x1, eq");                                         // would_block output = true only for nonblocking lock contention
    emitter.instruction("mov x0, #0");                                          // flock() returns false on failure
    emitter.label("__rt_flock_return");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return predicate

    // ================================================================
    // __rt_tmpfile: create an anonymous temp file.
    // Input:  none
    // Output: x0 = fd (or -1 on failure)
    // Frame:
    //   sp+ 0  : 32-byte template buffer (more than enough for /tmp/elephc-XXXXXX)
    //   sp+32  : x29 / x30
    // ================================================================
    let tmpl_buf = 32usize;
    let tmpl_save = tmpl_buf;
    let tmpl_frame = tmpl_buf + 16;
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for the next runtime helper
    emitter.comment("--- runtime: tmpfile ---");
    emitter.label_global("__rt_tmpfile");
    emitter.instruction(&format!("sub sp, sp, #{}", tmpl_frame));               // allocate frame + template buffer
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", tmpl_save));        // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", tmpl_save));               // establish new frame pointer
    emitter.adrp("x9", "_tmpfile_template");                                    // load page of the template literal
    emitter.add_lo12("x9", "x9", "_tmpfile_template");                          // resolve full address of the template literal
    emitter.instruction("ldp x10, x11, [x9]");                                  // load 16 bytes of the template
    emitter.instruction("stp x10, x11, [sp]");                                  // copy first 16 bytes onto the stack template
    emitter.instruction("ldr x10, [x9, #16]");                                  // load the remaining bytes (≤ 8) of the template
    emitter.instruction("str x10, [sp, #16]");                                  // copy the trailing bytes onto the stack template

    emitter.instruction("add x0, sp, #0");                                      // mkstemp template argument
    emitter.bl_c("mkstemp");                                                    // libc mkstemp() → fd (or -1)
    emitter.instruction("cmp w0, #0");                                          // did mkstemp return a negative C int?
    emitter.instruction("b.lt __rt_tmpfile_fail");                              // mkstemp failed
    emitter.instruction("sxtw x0, w0");                                         // normalize the C int fd into the runtime's 64-bit descriptor value
    emitter.instruction("str x0, [sp, #24]");                                   // preserve fd on the stack across the unlink call (x9–x15 are caller-saved)
    emitter.instruction("add x0, sp, #0");                                      // unlink path argument (the now-resolved template)
    emitter.bl_c("unlink");                                                     // libc unlink — file auto-deletes when fd closes
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload fd as the return value
    abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("strb wzr, [x9, x0]");                                  // clear stale EOF state for the temporary descriptor
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", tmpl_save));        // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", tmpl_frame));               // deallocate frame
    emitter.instruction("ret");                                                 // return fd

    emitter.label("__rt_tmpfile_fail");
    emitter.instruction("mov x0, #-1");                                         // tmpfile failure sentinel
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", tmpl_save));        // restore frame pointer and return address (failure path)
    emitter.instruction(&format!("add sp, sp, #{}", tmpl_frame));               // deallocate frame (failure path)
    emitter.instruction("ret");                                                 // return -1
}

/// Emits x86_64 Linux stream-extension runtime helpers (`__rt_fgetc`, `__rt_readfile`,
/// `__rt_fpassthru`, `__rt_flock`, `__rt_tmpfile`).
///
/// Called from `emit_streams_ext` when `emitter.target.arch == Arch::X86_64`.
fn emit_streams_ext_linux_x86_64(emitter: &mut Emitter) {
    // -- fgetc --
    emitter.blank();
    emitter.comment("--- runtime: fgetc ---");
    emitter.label_global("__rt_fgetc");
    emitter.instruction("mov rsi, 1");                                          // length = 1 (__rt_fread x86_64 ABI: rdi=fd, rsi=length)
    emitter.instruction("jmp __rt_fread");                                      // tail-call fread

    let buf_size = 4096usize;

    // -- readfile --
    emitter.blank();
    emitter.comment("--- runtime: readfile ---");
    emitter.label_global("__rt_readfile");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base
    emitter.instruction(&format!("sub rsp, {}", buf_size + 16));                // reserve frame for buffer + counters
    emitter.instruction("call __rt_cstr");                                      // path → C string in rax
    emitter.instruction("mov rdi, rax");                                        // first libc open arg
    emitter.instruction("xor esi, esi");                                        // O_RDONLY
    emitter.instruction("call open");                                           // libc open(path, O_RDONLY)
    emitter.instruction("cmp rax, 0");                                          // success?
    emitter.instruction("jl __rt_readfile_fail_x86");                           // failure → PHP false sentinel
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // total bytes copied = 0

    emitter.label("__rt_readfile_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction(&format!("mov rdx, {}", buf_size));                     // count
    emitter.instruction("call read");                                           // libc read()
    emitter.instruction("cmp rax, 0");                                          // bytes?
    emitter.instruction("jl __rt_readfile_read_error_x86");                     // read failure → return PHP's -1 byte count
    emitter.instruction("je __rt_readfile_done_x86");                           // EOF → stop
    emitter.instruction("add QWORD PTR [rbp - 16], rax");                       // total += bytes read
    emitter.instruction("mov rdx, rax");                                        // count to write
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction("call write");                                          // libc write(1, buf, n)
    emitter.instruction("jmp __rt_readfile_loop_x86");                          // continue

    emitter.label("__rt_readfile_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("call close");                                          // libc close
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return total
    emitter.instruction(&format!("add rsp, {}", buf_size + 16));                // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return total bytes

    emitter.label("__rt_readfile_read_error_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("call close");                                          // close fd after the failed read
    emitter.instruction("mov rax, -1");                                         // read failure sentinel, matching PHP's -1 byte count
    emitter.instruction(&format!("add rsp, {}", buf_size + 16));                // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return read failure sentinel

    emitter.label("__rt_readfile_fail_x86");
    emitter.instruction("mov rax, -2");                                         // open failure → PHP false sentinel
    emitter.instruction(&format!("add rsp, {}", buf_size + 16));                // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure sentinel

    // -- fpassthru --
    emitter.blank();
    emitter.comment("--- runtime: fpassthru ---");
    emitter.label_global("__rt_fpassthru");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base
    emitter.instruction(&format!("sub rsp, {}", buf_size + 16));                // reserve frame for buffer + counter
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save fd
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // total bytes copied = 0

    emitter.label("__rt_fpassthru_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction(&format!("mov rdx, {}", buf_size));                     // count
    emitter.instruction("call read");                                           // libc read
    emitter.instruction("cmp rax, 0");                                          // bytes?
    emitter.instruction("jl __rt_fpassthru_read_error_x86");                    // read failure → return PHP's -1 byte count
    emitter.instruction("je __rt_fpassthru_done_x86");                          // EOF → stop
    emitter.instruction("add QWORD PTR [rbp - 16], rax");                       // accumulate total bytes copied
    emitter.instruction("mov rdx, rax");                                        // count to write
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction("call write");                                          // libc write
    emitter.instruction("jmp __rt_fpassthru_loop_x86");                         // continue

    emitter.label("__rt_fpassthru_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload fd so feof() observes that passthru reached EOF
    emitter.instruction("lea r11, [rip + _eof_flags]");                         // materialize the eof-flag table for fpassthru completion
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // set _eof_flags[fd] after consuming the stream
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return total
    emitter.instruction(&format!("add rsp, {}", buf_size + 16));                // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return total

    emitter.label("__rt_fpassthru_read_error_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload fd so feof() observes the exhausted error state
    emitter.instruction("lea r11, [rip + _eof_flags]");                         // materialize the eof-flag table after fpassthru read failure
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // set _eof_flags[fd] after a failed read
    emitter.instruction("mov rax, -1");                                         // read failure sentinel, matching PHP's -1 byte count
    emitter.instruction(&format!("add rsp, {}", buf_size + 16));                // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return read failure sentinel

    // -- flock --
    emitter.blank();
    emitter.comment("--- runtime: flock ---");
    emitter.label_global("__rt_flock");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("mov rdi, rax");                                        // fd
    emitter.instruction("mov r8, rdx");                                         // copy op for masking
    emitter.instruction("and r8, 4");                                           // r8 = LOCK_NB bit
    emitter.instruction("and rdx, 3");                                          // rdx = base op (1/2/3)
    emitter.instruction("cmp rdx, 3");                                          // LOCK_UN?
    emitter.instruction("jne __rt_flock_done_translate_x86");                   // not LOCK_UN
    emitter.instruction("mov rdx, 8");                                          // POSIX LOCK_UN = 8
    emitter.label("__rt_flock_done_translate_x86");
    emitter.instruction("or rdx, r8");                                          // recombine LOCK_NB flag
    emitter.instruction("mov rsi, rdx");                                        // op into secondary libc arg
    emitter.instruction("call flock");                                          // libc flock(fd, op)
    emitter.instruction("cmp eax, 0");                                          // did libc flock() succeed?
    emitter.instruction("jne __rt_flock_fail_x86");                             // failed lock attempt: inspect errno for would-block state
    emitter.instruction("xor edx, edx");                                        // would_block output = false on success
    emitter.instruction("mov eax, 1");                                          // flock() returns true on success
    emitter.instruction("jmp __rt_flock_return_x86");                           // skip errno inspection after successful flock()
    emitter.label("__rt_flock_fail_x86");
    emitter.instruction("call __errno_location");                               // fetch thread-local errno storage after flock() failure
    emitter.instruction("mov r10d, DWORD PTR [rax]");                           // load errno value set by libc flock()
    emitter.instruction("cmp r10d, 11");                                        // compare errno with Linux EWOULDBLOCK/EAGAIN
    emitter.instruction("sete dl");                                             // would_block byte = true only for nonblocking lock contention
    emitter.instruction("movzx edx, dl");                                       // widen would_block output into rdx
    emitter.instruction("xor eax, eax");                                        // flock() returns false on failure
    emitter.label("__rt_flock_return_x86");
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return predicate

    // -- tmpfile --
    emitter.blank();
    emitter.comment("--- runtime: tmpfile ---");
    emitter.label_global("__rt_tmpfile");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 48");                                         // reserve template buffer plus fd spill slot
    emitter.instruction("lea rsi, [rip + _tmpfile_template]");                  // source pointer
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // load first 8 bytes
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // store first 8 bytes
    emitter.instruction("mov rax, QWORD PTR [rsi + 8]");                        // load next 8 bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // store next 8 bytes
    emitter.instruction("mov rax, QWORD PTR [rsi + 16]");                       // load remainder
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // store remainder
    emitter.instruction("lea rdi, [rbp - 32]");                                 // mkstemp template arg
    emitter.instruction("call mkstemp");                                        // libc mkstemp
    emitter.instruction("cmp eax, 0");                                          // did mkstemp return a negative C int?
    emitter.instruction("jl __rt_tmpfile_fail_x86");                            // mkstemp failed
    emitter.instruction("cdqe");                                                // normalize the C int fd into the runtime's 64-bit descriptor value
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve fd across unlink
    emitter.instruction("lea rdi, [rbp - 32]");                                 // unlink path
    emitter.instruction("call unlink");                                         // libc unlink — file auto-deletes on close
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return fd
    emitter.instruction("lea r10, [rip + _eof_flags]");                         // materialize the eof-flag table for the temporary descriptor
    emitter.instruction("mov BYTE PTR [r10 + rax], 0");                         // clear stale EOF state before returning the descriptor
    emitter.instruction("add rsp, 48");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return fd

    emitter.label("__rt_tmpfile_fail_x86");
    emitter.instruction("mov rax, -1");                                         // failure sentinel
    emitter.instruction("add rsp, 48");                                         // release frame (failure path)
    emitter.instruction("pop rbp");                                             // restore caller frame pointer (failure path)
    emitter.instruction("ret");                                                 // return -1
}
