//! Purpose:
//! Emits the `__rt_file_put_contents`, `__rt_cstr` runtime helper assembly for file put contents.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_file_put_contents` runtime helper for PHP's `file_put_contents()`.
///
/// Dispatches to the target-specific implementation:
/// - ARM64: `emit_file_put_contents_arm64` (default)
/// - x86_64 Linux: `emit_file_put_contents_linux_x86_64`
///
/// Two entry points share one body. `__rt_file_put_contents` keeps the original
/// two-argument ABI and writes with no PHP flags; `__rt_file_put_contents_flagged` takes
/// PHP's `$flags` word in an extra register. Splitting them this way leaves the six internal
/// callers of the original label — phar writes, `copy()` — untouched (issue #506).
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

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save data string for after cstr call --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save data ptr and len on stack
    emitter.instruction("str x5, [sp, #40]");                                   // save the PHP flags across the cstr call

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
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

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file_put_contents uses stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved pointers and lengths
    emitter.instruction("sub rsp, 48");                                         // reserve aligned stack space for data, path, fd, flags, and byte-count temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the data pointer while the filename is converted to a C string
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the data length while the filename is converted to a C string
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the PHP flags across the cstr call
    emitter.instruction("call __rt_cstr");                                      // convert the elephc filename in rax/rdx into a null-terminated C path in rax
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
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the opened file descriptor for the later write() and close() calls

    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the PHP flags
    emitter.instruction("test rax, 2");                                         // LOCK_EX is PHP constant 2
    emitter.instruction("je __rt_fpc_write_linux_x86_64");                      // no lock requested: write straight away
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the open fd to the lock helper
    emitter.instruction("mov rsi, 2");                                          // LOCK_EX, in the PHP numbering __rt_flock expects
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
