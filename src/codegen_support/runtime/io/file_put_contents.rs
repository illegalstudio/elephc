//! Purpose:
//! Emits the `__rt_file_put_contents`, `__rt_cstr` runtime helper assembly for file put contents.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_file_put_contents` runtime helper for PHP's `file_put_contents()`.
///
/// Dispatches to the target-specific implementation:
/// - ARM64: `emit_file_put_contents_arm64` (default)
/// - x86_64 Linux: `emit_file_put_contents_linux_x86_64`
///
/// Two entry points share one body. `__rt_file_put_contents` keeps the original
/// two-argument ABI and writes with no PHP flags; `__rt_file_put_contents_flagged` takes
/// PHP's `$flags` word in an extra register. Splitting them this way leaves the internal
/// callers of the original label — the phar archive writer and `copy()`, four across the two
/// targets — untouched (issue #506).
///
/// `FILE_APPEND` (8) selects `O_APPEND` over `O_TRUNC`; `LOCK_EX` (2) takes an exclusive
/// `flock` on the open descriptor, which `close()` then releases.
///
/// `LOCK_EX` WITHOUT `FILE_APPEND` opens with neither `O_TRUNC` nor `O_APPEND` and truncates
/// after the lock is held. That ordering is php-src's: it opens this combination in `'c'`
/// mode and calls `php_stream_truncate_set_size(stream, 0)` only once locked, because
/// truncating at open destroys a previous writer's contents while this writer is still
/// waiting for the lock. A REFUSED lock is a failed write — the descriptor is closed and
/// `-1` returned — rather than an unlocked write, and so is a refused TRUNCATION: writing
/// over a file that could not be emptied would report a byte count while leaving a stale
/// tail behind it.
///
/// # Input (ARM64 calling convention)
/// - x1/x2: filename string (pointer/length)
/// - x3/x4: data string (pointer/length)
/// - x5: PHP `$flags` (flagged entry point only)
///
/// # Output
/// - x0: bytes written on success, -1 on error
pub fn emit_file_put_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_put_contents_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: file_put_contents ---");
    emitter.label_global("__rt_file_put_contents");
    emitter.instruction("mov x5, #0");                                          // the two-argument form writes with no PHP flags
    emitter.instruction("b __rt_file_put_contents_flagged");                    // share one body with the flag-taking entry point

    emitter.blank();
    emitter.comment("--- runtime: file_put_contents (with PHP $flags) ---");
    emitter.label_global("__rt_file_put_contents_flagged");
    emit_refuse_data_scheme_aarch64(emitter);
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(-1),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_file_put_contents",
            name_len: 17,
            directory: false,
        },
    );

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save data string for after cstr call --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save data ptr and len on stack
    emitter.instruction("str x5, [sp, #40]");                                   // save the PHP flags across the cstr call

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_path_cstr");                                   // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- open the file --
    //
    // LOCK_EX without FILE_APPEND deliberately does NOT truncate at open. php-src opens
    // that combination in `'c'` mode and calls `php_stream_truncate_set_size(stream, 0)`
    // only after the lock is held, because truncating first destroys the previous writer's
    // contents while this one is still waiting for the lock.
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the PHP flags
    emitter.instruction(&format!("mov x1, #0x{:X}", emitter.platform.o_wronly_creat_trunc())); // O_WRONLY|O_CREAT|O_TRUNC
    emitter.instruction(&format!("mov x10, #0x{:X}", emitter.platform.o_wronly_creat_append())); // O_WRONLY|O_CREAT|O_APPEND
    emitter.instruction(&format!("mov x11, #0x{:X}", emitter.platform.o_wronly_creat())); // O_WRONLY|O_CREAT, php-src's 'c' mode
    emitter.instruction("tst x9, #8");                                          // FILE_APPEND is PHP constant 8
    emitter.instruction("csel x1, x10, x1, ne");                                // appending never truncates
    emitter.instruction("b.ne __rt_fpc_open");                                  // FILE_APPEND decided the flags
    emitter.instruction("tst x9, #2");                                          // LOCK_EX is PHP constant 2
    emitter.instruction("csel x1, x11, x1, ne");                                // defer the truncation until the lock is held
    emitter.label("__rt_fpc_open");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x2, #0x1A4");                                      // file mode 0644 (octal)
    emitter.syscall(5);
    // The open result was NEVER CHECKED. On macOS a failed open answers the ERRNO with the
    // carry set, so `file_put_contents("/no/such/dir/x", $payload)` wrote the payload to
    // descriptor 2 — the caller's stderr — and reported the byte count as a success. php warns
    // and answers false.
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux reports open failure as a negative result
    }
    let opened_branch = emitter.platform.branch_on_syscall_success("__rt_fpc_opened");
    emitter.instruction(&opened_branch);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("neg x3, x0");                                      // Linux answers -errno
    } else {
        emitter.instruction("mov x3, x0");                                      // macOS answers the errno itself
    }
    emitter.instruction("ldr x2, [sp, #0]");                                    // the null-terminated path
    abi::emit_symbol_address(emitter, "x0", "_diag_open_failed_fpc_prefix");
    emitter.instruction(&format!("mov x1, #{}", "Warning: file_put_contents(".len()));
    // Prefer a name a delegating builtin published for the duration of its call. The
    // two values are loaded SEPARATELY: materializing a symbol borrows the scratch
    // register, so a length held there would not survive the pointer load.
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_open_diag_prefix_len", 0);
    emitter.instruction("cbz x9, __rt_fpc_open_named");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_rt_open_diag_prefix", 0);
    abi::emit_load_symbol_to_reg(emitter, "x1", "_rt_open_diag_prefix_len", 0);
    emitter.label("__rt_fpc_open_named");
    emitter.instruction("bl __rt_open_failed_warning");
    emitter.instruction("mov x0, #-1");                                         // php answers false for a path it cannot open
    emitter.instruction("b __rt_fpc_return");
    emitter.label("__rt_fpc_opened");
    emitter.instruction("str x0, [sp, #8]");                                    // save fd on stack

    // -- take an exclusive lock when LOCK_EX is set, as PHP does --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the PHP flags
    emitter.instruction("tst x9, #2");                                          // LOCK_EX is PHP constant 2
    emitter.instruction("b.eq __rt_fpc_write");                                 // no lock requested: write straight away
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass the open fd to the lock helper
    emitter.instruction("mov x1, #2");                                          // LOCK_EX, in the PHP numbering __rt_flock expects
    emitter.instruction("bl __rt_flock");                                       // block until the exclusive lock is held
    emitter.instruction("cbz x0, __rt_fpc_lock_failed");                        // PHP reports a failed lock as a failed write

    // -- truncate under the lock, which is what `'c'` mode defers --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the PHP flags
    emitter.instruction("tst x9, #8");                                          // FILE_APPEND is PHP constant 8
    emitter.instruction("b.ne __rt_fpc_write");                                 // appending has nothing to truncate
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.instruction("mov x1, #0");                                          // truncate to zero, as `'w'` mode would have
    emitter.instruction("bl __rt_ftruncate");                                   // now safe: no other writer holds the lock
    emitter.instruction("cbz x0, __rt_fpc_lock_failed");                        // a refused truncation would leave a stale tail
    emitter.label("__rt_fpc_write");

    // -- write data to file --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload data pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload data length
    emitter.syscall(4);
    emitter.instruction("str x0, [sp, #32]");                                   // save bytes written

    // -- close the file --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.syscall(6);

    // -- return bytes written --
    emitter.instruction("ldr x0, [sp, #32]");                                   // return bytes written
    emitter.instruction("b __rt_fpc_return");                                   // skip the lock-failure exit

    // -- a refused lock is a failed write, and the file is left untouched --
    emitter.label("__rt_fpc_lock_failed");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.syscall(6);
    emitter.instruction("mov x0, #-1");                                         // file_put_contents()'s failure result

    emitter.label("__rt_fpc_return");
    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux implementation of `__rt_file_put_contents`.
///
/// Uses the System V AMD64 ABI: rdi/rsi/rdx for the first three integer arguments.
/// Calls `__rt_cstr` to convert the filename, then libc `open`, `write`, and `close`.
///
/// The two runtime helpers it calls do NOT follow that ABI, and their ARM64 siblings do take
/// x0/x1, so the mismatch is invisible on the arch this is usually developed on:
/// `__rt_flock` reads its fd from `rax` and its lock op from `rdx`, and `__rt_ftruncate` reads
/// its fd from `rax` with the size already in `rsi`. Passing `rdi`/`rsi` to `__rt_flock` here
/// made every `LOCK_EX` write return `-1` on linux-x86_64 while linux-aarch64 and
/// macos-aarch64 passed.
///
/// # Input (System V AMD64 ABI)
/// - rdi/rsi: data string (pointer/length)
/// - rdx/rcx: filename string (pointer/length)
///
/// # Output
/// - rax: bytes written on success, -1 on error
fn emit_file_put_contents_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_put_contents ---");
    emitter.label_global("__rt_file_put_contents");
    emitter.instruction("xor r8d, r8d");                                        // the two-argument form writes with no PHP flags
    emitter.instruction("jmp __rt_file_put_contents_flagged");                  // share one body with the flag-taking entry point

    emitter.blank();
    emitter.comment("--- runtime: file_put_contents (with PHP $flags) ---");
    emitter.label_global("__rt_file_put_contents_flagged");
    emit_refuse_data_scheme_x86_64(emitter);
    // php locates a wrapper for every path; a bare one is the plain-files wrapper.
    super::fopen::emit_refuse_when_file_wrapper_disabled_saying(
        emitter,
        super::fopen::DisabledWrapperAnswer::Predicate(-1),
        super::fopen::DisabledWrapperNotice::FailedToOpen {
            name_symbol: "_uww_name_file_put_contents",
            name_len: 17,
            directory: false,
        },
    );

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file_put_contents uses stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved pointers and lengths
    emitter.instruction("sub rsp, 48");                                         // reserve aligned stack space for data, path, fd, flags, and byte-count temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the data pointer while the filename is converted to a C string
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the data length while the filename is converted to a C string
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the PHP flags across the cstr call
    emitter.instruction("call __rt_path_cstr");                                 // convert the elephc filename in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the C filename pointer for the later open() call

    // LOCK_EX without FILE_APPEND deliberately does NOT truncate at open; php-src opens that
    // combination in `'c'` mode and truncates only once the lock is held.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the C filename pointer as the first libc open() argument
    emitter.instruction(&format!("mov rsi, 0x{:X}", emitter.platform.o_wronly_creat_trunc())); // pass O_WRONLY|O_CREAT|O_TRUNC as the open() flags
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the PHP flags
    emitter.instruction("test rax, 8");                                         // FILE_APPEND is PHP constant 8
    emitter.instruction("jz __rt_fpc_open_lock_linux_x86_64");                  // no append requested: consider the lock mode
    emitter.instruction(&format!("mov rsi, 0x{:X}", emitter.platform.o_wronly_creat_append())); // pass O_WRONLY|O_CREAT|O_APPEND instead
    emitter.instruction("jmp __rt_fpc_open_linux_x86_64");                      // FILE_APPEND decided the flags
    emitter.label("__rt_fpc_open_lock_linux_x86_64");
    emitter.instruction("test rax, 2");                                         // LOCK_EX is PHP constant 2
    emitter.instruction("jz __rt_fpc_open_linux_x86_64");                       // no lock: the truncating flags stand
    emitter.instruction(&format!("mov rsi, 0x{:X}", emitter.platform.o_wronly_creat())); // O_WRONLY|O_CREAT, php-src's 'c' mode
    emitter.label("__rt_fpc_open_linux_x86_64");
    emitter.instruction("mov rdx, 0x1A4");                                      // pass mode 0644 for newly created files
    emitter.instruction("call open");                                           // open the destination file through libc open()
    // See the AArch64 half: the open result was never checked, so an unopenable path wrote the
    // payload through a garbage descriptor and reported success. php warns and answers false.
    emitter.instruction("test eax, eax");                                       // libc open reports failure as a negative int
    emitter.instruction("jns __rt_fpc_opened_x");
    emitter.instruction("call __errno_location");
    emitter.instruction("movsxd rcx, DWORD PTR [rax]");                         // the errno to describe
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // the null-terminated path
    abi::emit_symbol_address(emitter, "rdi", "_diag_open_failed_fpc_prefix");
    emitter.instruction(&format!("mov esi, {}", "Warning: file_put_contents(".len()));
    // See the AArch64 arm: load both values from their slots, never park one in scratch.
    abi::emit_load_symbol_to_reg(emitter, "r11", "_rt_open_diag_prefix_len", 0);
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_fpc_open_named_x86");
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_rt_open_diag_prefix", 0);
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_rt_open_diag_prefix_len", 0);
    emitter.label("__rt_fpc_open_named_x86");
    emitter.instruction("call __rt_open_failed_warning");
    emitter.instruction("mov rax, -1");                                         // php answers false for a path it cannot open
    emitter.instruction("jmp __rt_fpc_return_linux_x86_64");
    emitter.label("__rt_fpc_opened_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the opened file descriptor for the later write() and close() calls

    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the PHP flags
    emitter.instruction("test rax, 2");                                         // LOCK_EX is PHP constant 2
    emitter.instruction("je __rt_fpc_write_linux_x86_64");                      // no lock requested: write straight away
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // __rt_flock takes its fd in rax, NOT in the SysV first argument register
    emitter.instruction("mov rdx, 2");                                          // LOCK_EX in the PHP numbering, in the register __rt_flock reads the op from
    emitter.instruction("call __rt_flock");                                     // block until the exclusive lock is held
    emitter.instruction("test rax, rax");                                       // did the lock succeed?
    emitter.instruction("jz __rt_fpc_lock_failed_linux_x86_64");                // PHP reports a failed lock as a failed write

    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the PHP flags
    emitter.instruction("test rax, 8");                                         // FILE_APPEND is PHP constant 8
    emitter.instruction("jnz __rt_fpc_write_linux_x86_64");                     // appending has nothing to truncate
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // pass the fd in the ftruncate helper's input register
    emitter.instruction("xor esi, esi");                                        // truncate to zero, as `'w'` mode would have
    emitter.instruction("call __rt_ftruncate");                                 // now safe: no other writer holds the lock
    emitter.instruction("test rax, rax");                                       // did the truncation succeed?
    emitter.instruction("jz __rt_fpc_lock_failed_linux_x86_64");                // a refused truncation would leave a stale tail
    emitter.label("__rt_fpc_write_linux_x86_64");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc write() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // pass the source data pointer as the second libc write() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the source data length as the third libc write() argument
    emitter.instruction("call write");                                          // write the requested bytes into the opened destination file
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the number of written bytes for the final return value

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc close() argument
    emitter.instruction("call close");                                          // close the destination file after the write completes

    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the number of bytes reported by libc write()
    emitter.instruction("jmp __rt_fpc_return_linux_x86_64");                    // skip the lock-failure exit

    emitter.label("__rt_fpc_lock_failed_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the file descriptor
    emitter.instruction("call close");                                          // release the descriptor the refused lock leaves open
    emitter.instruction("mov rax, -1");                                         // file_put_contents()'s failure result

    emitter.label("__rt_fpc_return_linux_x86_64");
    emitter.instruction("add rsp, 48");                                         // release the aligned stack locals used by file_put_contents
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller with the write byte count in rax
}

/// Refuses a `data:` path the way php does, before the plain-file open is ever attempted.
///
/// php locates the wrapper first and the data wrapper OPENS in any mode — it simply has no write
/// function, so the write is what fails. MEASURED on `php -n` 8.5.6, both spellings:
///
/// ```text
///     file_put_contents('data://text/plain,cccc', 'x')  Notice: … Stream is not writable / false
///     file_put_contents('data:text/plain,cccc', 'x')    the same
/// ```
///
/// elephc never consulted a wrapper here and opened the whole URL as a FILENAME, so it answered
/// `Warning: file_put_contents(data://text/plain,cccc): Failed to open stream: No such file or
/// directory` — a wrong reason for a stream that opens perfectly well.
///
/// The scheme is matched case-SENSITIVELY, because php is: `DATA://` and `Data://` are not the
/// data wrapper for either implementation (measured on both).
///
/// ⚠️ Only `data:` is routed. Every other wrapper still reaches the plain-file open, and each has
/// its own php answer; this is the one the corpus names.
fn emit_refuse_data_scheme_aarch64(emitter: &mut Emitter) {
    emitter.instruction("cmp x2, #5");                                          // "data:" is five bytes
    emitter.instruction("b.lt __rt_fpc_not_data");                              // too short to be the data wrapper
    abi::emit_symbol_address(emitter, "x9", "_data_n_prefix");                  // "data://" — the first five bytes are the scheme
    emitter.instruction("mov x10, #0");                                         // compare cursor
    emitter.label("__rt_fpc_data_scan");
    emitter.instruction("cmp x10, #5");                                         // compared the whole scheme?
    emitter.instruction("b.ge __rt_fpc_is_data");                               // it is the data wrapper
    emitter.instruction("ldrb w11, [x1, x10]");                                 // the path byte
    emitter.instruction("ldrb w12, [x9, x10]");                                 // the scheme byte
    emitter.instruction("cmp w11, w12");
    emitter.instruction("b.ne __rt_fpc_not_data");                              // some other wrapper, or a plain path
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("b __rt_fpc_data_scan");
    emitter.label("__rt_fpc_is_data");
    emitter.instruction("sub sp, sp, #16");                                     // frame for the diagnostic call
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    crate::codegen_support::runtime::io::emit_announce_read_fn_name(
        emitter,
        "_uww_name_file_put_contents",
        17,
    );                                                                          // php names the builtin the user wrote
    emitter.instruction("bl __rt_not_writable_notice");
    crate::codegen_support::runtime::io::emit_clear_read_fn_name(emitter);      // a later fwrite() is its own name again
    emitter.instruction("mov x0, #-1");                                         // negative: the caller boxes PHP false
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");
    emitter.label("__rt_fpc_not_data");
}

/// The x86_64 counterpart of [`emit_refuse_data_scheme_aarch64`]; the filename is in `rax`/`rdx`.
fn emit_refuse_data_scheme_x86_64(emitter: &mut Emitter) {
    emitter.instruction("cmp rdx, 5");                                          // "data:" is five bytes
    emitter.instruction("jl __rt_fpc_not_data_x86");                            // too short to be the data wrapper
    abi::emit_symbol_address(emitter, "r9", "_data_n_prefix");                  // "data://" — the first five bytes are the scheme
    emitter.instruction("xor r10, r10");                                        // compare cursor
    emitter.label("__rt_fpc_data_scan_x86");
    emitter.instruction("cmp r10, 5");                                          // compared the whole scheme?
    emitter.instruction("jge __rt_fpc_is_data_x86");                            // it is the data wrapper
    emitter.instruction("movzx r11d, BYTE PTR [rax + r10]");                    // the path byte
    emitter.instruction("movzx ecx, BYTE PTR [r9 + r10]");                      // the scheme byte (r8 carries $flags)
    emitter.instruction("cmp r11b, cl");
    emitter.instruction("jne __rt_fpc_not_data_x86");                           // some other wrapper, or a plain path
    emitter.instruction("add r10, 1");
    emitter.instruction("jmp __rt_fpc_data_scan_x86");
    emitter.label("__rt_fpc_is_data_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    crate::codegen_support::runtime::io::emit_announce_read_fn_name(
        emitter,
        "_uww_name_file_put_contents",
        17,
    );                                                                          // php names the builtin the user wrote
    emitter.instruction("call __rt_not_writable_notice");
    crate::codegen_support::runtime::io::emit_clear_read_fn_name(emitter);      // a later fwrite() is its own name again
    emitter.instruction("mov rax, -1");                                         // negative: the caller boxes PHP false
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
    emitter.label("__rt_fpc_not_data_x86");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// `__rt_flock` and `__rt_ftruncate` do NOT take the platform's first argument registers on
    /// x86_64: both read their fd from `rax`, `__rt_flock` reads its lock op from `rdx`, and
    /// `__rt_ftruncate` expects the size already in `rsi`. Their ARM64 siblings DO take x0/x1,
    /// so a caller written on ARM64 and mirrored register-for-register into the x86_64 emitter
    /// assembles, links, and locks whatever happened to be in `rax`.
    ///
    /// That is what happened: `file_put_contents($f, …, LOCK_EX)` returned `-1` and wrote
    /// nothing on linux-x86_64 while both aarch64 targets passed. Asserting the call sites here
    /// catches it without an x86_64 host.
    #[test]
    fn test_x86_64_lock_path_uses_the_registers_its_helpers_read() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_file_put_contents(&mut emitter);
        let asm = emitter.output();

        let before_flock = asm
            .split("call __rt_flock")
            .next()
            .expect("the x86_64 emitter must call __rt_flock");
        assert!(
            before_flock.ends_with("mov rax, QWORD PTR [rbp - 32]\n    mov rdx, 2\n    "),
            "the fd must reach __rt_flock in rax and the op in rdx, not in rdi/rsi"
        );

        let before_ftruncate = asm
            .split("call __rt_ftruncate")
            .next()
            .expect("the x86_64 emitter must call __rt_ftruncate");
        assert!(
            before_ftruncate.ends_with("mov rax, QWORD PTR [rbp - 32]\n    xor esi, esi\n    "),
            "the fd must reach __rt_ftruncate in rax with the size in rsi"
        );
    }

    /// The ARM64 siblings take x0/x1, and the lock op is PHP's `LOCK_EX` (2) rather than the
    /// POSIX value — `__rt_flock` does that translation itself, and only for `LOCK_UN`.
    #[test]
    fn test_arm64_lock_path_uses_the_registers_its_helpers_read() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_file_put_contents(&mut emitter);
        let asm = emitter.output();

        let before_flock = asm
            .split("bl __rt_flock")
            .next()
            .expect("the ARM64 emitter must call __rt_flock");
        assert!(
            before_flock.ends_with("ldr x0, [sp, #8]\n    mov x1, #2\n    "),
            "the fd must reach __rt_flock in x0 and the op in x1"
        );

        let before_ftruncate = asm
            .split("bl __rt_ftruncate")
            .next()
            .expect("the ARM64 emitter must call __rt_ftruncate");
        assert!(
            before_ftruncate.ends_with("ldr x0, [sp, #8]\n    mov x1, #0\n    "),
            "the fd must reach __rt_ftruncate in x0 with the size in x1"
        );
    }

    /// A refused lock and a refused truncation take the SAME exit on both targets: close the
    /// descriptor and return `-1`. An unlocked write, or a write over a file that could not be
    /// emptied, would report a byte count while leaving a stale tail behind it.
    #[test]
    fn test_both_targets_fail_the_write_when_the_lock_or_the_truncation_is_refused() {
        for (target, label) in [
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__rt_fpc_lock_failed_linux_x86_64",
            ),
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "__rt_fpc_lock_failed",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_file_put_contents(&mut emitter);
            let asm = emitter.output();
            // Once as the label, twice as a branch target: the refused lock and the refused
            // truncation.
            assert_eq!(
                asm.matches(label).count(),
                3,
                "both refusals must reach {} on {:?}",
                label,
                target.arch
            );
        }
    }
}
