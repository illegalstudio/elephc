//! Purpose:
//! Emits stream-extension runtime helpers (`__rt_fgetc`, `__rt_readfile`,
//! `__rt_fpassthru`, `__rt_flock`, `__rt_tmpfile`).
//! Bridges PHP stream-side builtins to libc/syscalls for ARM64 (Darwin/Linux)
//! and Linux x86_64.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
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
//! - Unix `__rt_tmpfile` routes `mkstemp` through `Emitter::emit_call_c` and
//!   immediately unlinks the path. Windows instead uses the system temp
//!   directory plus `GetTempFileNameW` and reopens with
//!   `FILE_FLAG_DELETE_ON_CLOSE`, matching php-src's close-time deletion.

use crate::codegen_support::{
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
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // buffer pointer
    emitter.instruction("mov x2, x9");                                          // length to write
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)
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
    emitter.instruction(&format!("add x1, sp, #{}", buf_off));                  // buffer pointer
    emitter.instruction("mov x2, x9");                                          // length
    emitter.instruction("bl __rt_vd_write");                                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)
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
        Platform::Windows => "__errno_location", // Windows shims against msvcrt errno
    };
    let would_block_errno = match emitter.platform {
        Platform::MacOS => 35,
        Platform::Linux => 11,
        Platform::Windows => 11, // EAGAIN — msvcrt uses the POSIX value via the shim
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
    abi::emit_symbol_address(emitter, "x9", "_tmpfile_template");               // load page of the template literal
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
    emitter.instruction("cmp eax, 0");                                          // did libc open() return a negative C int?
    emitter.instruction("jl __rt_readfile_fail_x86");                           // failure → PHP false sentinel
    emitter.instruction("cdqe");                                                // normalize the successful C int fd into a 64-bit runtime descriptor
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
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)
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
    if emitter.target.platform == crate::codegen_support::platform::Platform::Windows {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore the opaque descriptor before resetting timeout metadata
        emitter.instruction("call __rt_win_stream_clear_timed_out");            // each new passthru read starts with timed_out=false
    }
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction(&format!("mov rdx, {}", buf_size));                     // count
    emitter.instruction("call read");                                           // libc read
    emitter.instruction("cmp rax, 0");                                          // bytes?
    emitter.instruction("jl __rt_fpassthru_read_error_x86");                    // read failure → return PHP's -1 byte count
    emitter.instruction("je __rt_fpassthru_done_x86");                          // EOF → stop
    emitter.instruction("add QWORD PTR [rbp - 16], rax");                       // accumulate total bytes copied
    emitter.instruction("mov rdx, rax");                                        // count to write
    emitter.instruction(&format!("lea rsi, [rbp - {}]", buf_size + 16));        // buffer
    emitter.instruction("call __rt_vd_write");                                  // write rsi/rdx through the ob/web-aware stdout sink (register-preserving)
    emitter.instruction("jmp __rt_fpassthru_loop_x86");                         // continue

    emitter.label("__rt_fpassthru_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload fd so feof() observes that passthru reached EOF
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov rdi, r10");                                    // pass the opaque Windows descriptor to the slot registry
        emitter.instruction("call __rt_win_stream_slot");                       // obtain a bounded EOF-table slot
        emitter.instruction("mov r10, rax");                                    // table indexing uses the compact slot, never a raw SOCKET
    }
    abi::emit_symbol_address(emitter, "r11", "_eof_flags");                     // materialize the eof-flag table for fpassthru completion
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // set EOF on the compact stream slot after consuming the stream
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return total
    emitter.instruction(&format!("add rsp, {}", buf_size + 16));                // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return total

    emitter.label("__rt_fpassthru_read_error_x86");
    if emitter.target.platform == crate::codegen_support::platform::Platform::Windows {
        emitter.instruction("call __errno_location");                           // inspect the POSIX-mapped Winsock failure before setting EOF
        emitter.instruction("cmp DWORD PTR [rax], 110");                        // ETIMEDOUT is a retryable stream timeout, not EOF
        emitter.instruction("jne __rt_fpassthru_error_eof_x86");                // preserve legacy EOF behavior for non-timeout failures
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // restore the opaque stream descriptor for timeout metadata
        emitter.instruction("call __rt_win_stream_mark_timed_out");             // expose the timeout without changing EOF state
        emitter.instruction("mov rax, -1");                                     // retain fpassthru's failure sentinel after a timeout
        emitter.instruction(&format!("add rsp, {}", buf_size + 16));            // release the passthru frame without touching EOF
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return the timeout failure sentinel
        emitter.label("__rt_fpassthru_error_eof_x86");
    }
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload fd so feof() observes the exhausted error state
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov rdi, r10");                                    // pass the opaque Windows descriptor to the slot registry
        emitter.instruction("call __rt_win_stream_slot");                       // obtain a bounded EOF-table slot
        emitter.instruction("mov r10, rax");                                    // table indexing uses the compact slot, never a raw SOCKET
    }
    abi::emit_symbol_address(emitter, "r11", "_eof_flags");                     // materialize the eof-flag table after fpassthru read failure
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // set EOF on the compact stream slot after a failed read
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

    if emitter.target.platform == Platform::Windows {
        emit_tmpfile_windows_x86_64(emitter);
        return;
    }

    // -- tmpfile --
    emitter.blank();
    emitter.comment("--- runtime: tmpfile ---");
    emitter.label_global("__rt_tmpfile");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame
    emitter.instruction("sub rsp, 48");                                         // reserve template buffer plus fd spill slot
    abi::emit_symbol_address(emitter, "rsi", "_tmpfile_template");              // source pointer
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // load first 8 bytes
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // store first 8 bytes
    emitter.instruction("mov rax, QWORD PTR [rsi + 8]");                        // load next 8 bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // store next 8 bytes
    emitter.instruction("mov rax, QWORD PTR [rsi + 16]");                       // load remainder
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // store remainder
    emitter.instruction("lea rdi, [rbp - 32]");                                 // mkstemp template arg
    emitter.emit_call_c("mkstemp");                                             // Windows: exclusive CreateFileW-backed temporary-file shim
    emitter.instruction("cmp eax, 0");                                          // did mkstemp return a negative C int?
    emitter.instruction("jl __rt_tmpfile_fail_x86");                            // mkstemp failed
    emitter.instruction("cdqe");                                                // normalize the C int fd into the runtime's 64-bit descriptor value
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve fd across unlink
    emitter.instruction("lea rdi, [rbp - 32]");                                 // unlink path
    emitter.instruction("call unlink");                                         // libc unlink — file auto-deletes on close
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return fd
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov rdi, rax");                                    // pass temporary CRT descriptor to the slot registry
        emitter.instruction("call __rt_win_stream_slot");                       // obtain a bounded EOF-table slot
        emitter.instruction("mov r11, rax");                                    // retain compact slot for state initialization
        abi::emit_symbol_address(emitter, "r10", "_eof_flags");                 // materialize the eof-flag table for the temporary descriptor slot
        emitter.instruction("mov BYTE PTR [r10 + r11], 0");                     // clear stale EOF state without raw descriptor indexing
        emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // return the original temporary CRT descriptor
    } else {
        abi::emit_symbol_address(emitter, "r10", "_eof_flags");                 // materialize the eof-flag table for the temporary descriptor
        emitter.instruction("mov BYTE PTR [r10 + rax], 0");                     // clear stale EOF state before returning the descriptor
    }
    emitter.instruction("add rsp, 48");                                         // release frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return fd

    emitter.label("__rt_tmpfile_fail_x86");
    emitter.instruction("mov rax, -1");                                         // failure sentinel
    emitter.instruction("add rsp, 48");                                         // release frame (failure path)
    emitter.instruction("pop rbp");                                             // restore caller frame pointer (failure path)
    emitter.instruction("ret");                                                 // return -1
}

/// Emits the Windows x86_64 `tmpfile()` path using the configured system temp
/// directory and a delete-on-close Win32 handle adopted by the CRT.
fn emit_tmpfile_windows_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: tmpfile (Windows delete-on-close) ---");
    emitter.label_global("__rt_tmpfile");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for wide path buffers
    emitter.instruction("sub rsp, 8320");                                       // Win64 call area below two php-src-sized UTF-16 path buffers and spill slots
    emitter.instruction("mov DWORD PTR [rbp - 32], 0x006c0065");                // UTF-16 prefix bytes for "el"
    emitter.instruction("mov DWORD PTR [rbp - 28], 0x00000070");                // UTF-16 prefix tail "p" plus terminator
    emitter.instruction("mov ecx, 2048");                                       // PHP_WIN32_IOUTIL_MAXPATHLEN UTF-16 code units
    emitter.instruction("lea rdx, [rbp - 4128]");                               // writable php-src-sized temp-directory buffer
    emitter.instruction("call GetTempPathW");                                   // resolve TMP/TEMP or the Windows directory fallback
    emitter.instruction("test eax, eax");                                       // did Windows return a temp directory?
    emitter.instruction("jz __rt_tmpfile_native_fail_win");                     // publish GetLastError and return false
    emitter.instruction("cmp eax, 2048");                                       // did the configured path exceed the php-src-sized buffer?
    emitter.instruction("jae __rt_tmpfile_range_fail_win");                     // reject truncation with ERANGE
    emitter.instruction("lea rcx, [rbp - 4128]");                               // existing system temp directory
    emitter.instruction("lea rdx, [rbp - 32]");                                 // three-character UTF-16 prefix
    emitter.instruction("xor r8d, r8d");                                        // let Windows choose a unique numeric suffix
    emitter.instruction("lea r9, [rbp - 8224]");                                // receive the created temporary path
    emitter.instruction("call GetTempFileNameW");                               // create a unique file in the configured temp directory
    emitter.instruction("test eax, eax");                                       // did unique-file creation succeed?
    emitter.instruction("jz __rt_tmpfile_native_fail_win");                     // publish GetLastError and return false

    // -- reopen the named file with delete-on-close ownership --
    emitter.instruction("lea rcx, [rbp - 8224]");                               // unique UTF-16 temporary path
    emitter.instruction("mov rdx, 0xC0010000");                                 // GENERIC_READ | GENERIC_WRITE | DELETE
    emitter.instruction("mov r8d, 7");                                          // share read, write, and delete while the stream is alive
    emitter.instruction("xor r9d, r9d");                                        // default security and non-inheritable handle
    emitter.instruction("mov QWORD PTR [rsp + 32], 3");                         // OPEN_EXISTING file created by GetTempFileNameW
    emitter.instruction("mov QWORD PTR [rsp + 40], 0x04000100");                // FILE_FLAG_DELETE_ON_CLOSE | FILE_ATTRIBUTE_TEMPORARY
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // no template handle
    emitter.instruction("call CreateFileW");                                    // acquire the stream handle with close-time deletion
    emitter.instruction("cmp rax, -1");                                         // INVALID_HANDLE_VALUE?
    emitter.instruction("je __rt_tmpfile_open_fail_win");                       // remove the named file and publish the native error
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the owned Win32 handle until CRT adoption
    emitter.instruction("mov rcx, rax");                                        // handle transferred to _open_osfhandle on success
    emitter.instruction("mov edx, 0x8002");                                     // _O_BINARY | _O_RDWR
    emitter.instruction("call _open_osfhandle");                                // expose the delete-on-close handle as a CRT descriptor
    emitter.instruction("cmp eax, -1");                                         // did CRT descriptor allocation fail?
    emitter.instruction("je __rt_tmpfile_crt_fail_win");                        // close the still-owned handle and report EMFILE
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the CRT descriptor across slot lookup
    emitter.instruction("mov rdi, rax");                                        // opaque descriptor for bounded stream state
    emitter.instruction("call __rt_win_stream_slot");                           // allocate or recover the compact stream-state slot
    emitter.instruction("mov r11, rax");                                        // retain the slot while materializing the EOF table
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");                    // bounded EOF-state table
    emitter.instruction("mov BYTE PTR [r10 + r11], 0");                         // a fresh temporary stream starts before EOF
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the adopted CRT descriptor
    emitter.instruction("add rsp, 8320");                                       // release wide buffers and Win32 call staging
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the writable temporary resource descriptor

    emitter.label("__rt_tmpfile_open_fail_win");
    emitter.instruction("call GetLastError");                                   // preserve CreateFileW failure before cleanup
    emitter.instruction("mov DWORD PTR [rbp - 24], eax");                       // native open error
    emitter.instruction("lea rcx, [rbp - 8224]");                               // named file created by GetTempFileNameW
    emitter.instruction("call DeleteFileW");                                    // avoid leaking a failed temporary-file path
    emitter.instruction("mov eax, DWORD PTR [rbp - 24]");                       // restore the original CreateFileW error
    emitter.instruction("jmp __rt_tmpfile_translate_fail_win");                 // publish the preserved error

    emitter.label("__rt_tmpfile_crt_fail_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // handle remains owned when CRT adoption fails
    emitter.instruction("call CloseHandle");                                    // close and trigger FILE_FLAG_DELETE_ON_CLOSE
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 24");                // EMFILE: CRT descriptor table exhausted
    emitter.instruction("jmp __rt_tmpfile_fail_win");                           // return the failure sentinel

    emitter.label("__rt_tmpfile_native_fail_win");
    emitter.instruction("call GetLastError");                                   // obtain the failed temp-path API error
    emitter.label("__rt_tmpfile_translate_fail_win");
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain the native diagnostic code
    emitter.instruction("call __rt_win32_errno_from_code");                     // map the Win32 error into POSIX errno space
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish the temporary-file failure
    emitter.instruction("jmp __rt_tmpfile_fail_win");                           // return false

    emitter.label("__rt_tmpfile_range_fail_win");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 34");                // ERANGE: configured temp path exceeds MAX_PATH
    emitter.label("__rt_tmpfile_fail_win");
    emitter.instruction("mov rax, -1");                                         // tmpfile failure sentinel
    emitter.instruction("add rsp, 8320");                                       // release wide buffers and Win32 call staging
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return false through the builtin boxing path
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies Windows temp files use the configured system directory and
    /// delete-on-close ownership instead of the Unix `/tmp` unlink strategy.
    #[test]
    fn windows_tmpfile_uses_system_temp_and_delete_on_close() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_streams_ext(&mut emitter);
        let asm = emitter.output();
        let section = asm
            .split("__rt_tmpfile:\n")
            .nth(1)
            .expect("missing Windows tmpfile helper");
        assert!(section.contains("call GetTempPathW"));
        assert!(section.contains("call GetTempFileNameW"));
        assert!(section.contains("sub rsp, 8320"));
        assert!(section.contains("mov QWORD PTR [rsp + 40], 0x04000100"));
        assert!(section.contains("call _open_osfhandle"));
        assert!(section.contains("cmp eax, -1"));
        assert!(!section.contains("call __rt_sys_mkstemp"));
        assert!(!section.contains("call unlink"));
    }
}
