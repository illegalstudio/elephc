//! Purpose:
//! Emits the `__rt_file_put_contents`, `__rt_cstr` runtime helper assembly for file put contents.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Mirrors `_diag_file_put_contents_failed_msg` in the fixed runtime data; the length
/// immediate passed to `__rt_diag_warning` is derived from it.
pub(crate) const FILE_PUT_CONTENTS_FAILED_WARNING: &str =
    "Warning: file_put_contents(): Failed to open stream\n";

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
    emitter.comment("--- runtime: file_put_contents (internal, no warning) ---");
    emitter.label_global("__rt_file_put_contents_quiet");
    emitter.instruction("mov x5, #0");                                          // internal callers write with no PHP flags
    emitter.instruction("mov x6, #1");                                          // the caller reports its own open failure
    emitter.instruction("b __rt_fpc_body");                                     // share the flagged body

    emitter.blank();
    emitter.comment("--- runtime: file_put_contents (with PHP $flags) ---");
    emitter.label_global("__rt_file_put_contents_flagged");
    emitter.instruction("mov x6, #0");                                          // PHP-visible calls warn when the open fails
    emitter.instruction("b __rt_fpc_body");                                     // share the body (cross-atom safe: global target)

    // The body label is GLOBAL: on Mach-O each global symbol starts its own atom, and a
    // branch to a local label in another atom carries no relocation, so dead-strip could
    // drop a body only the quiet entry referenced.
    emitter.label_global("__rt_fpc_body");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- save data string for after cstr call --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save data ptr and len on stack
    emitter.instruction("str x5, [sp, #40]");                                   // save the PHP flags across the cstr call
    emitter.instruction("str x6, [sp, #48]");                                   // save the quiet flag across the cstr call

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
    // A failed open() leaves the errno (Darwin, carry set) or -errno (Linux) in x0. Using
    // that as a descriptor wrote the payload to stderr or to whatever fd matched the errno.
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result is -errno
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fpc_opened")); // only a real descriptor reaches write()
    emitter.instruction("b __rt_fpc_open_failed");                              // open() failed: report false without writing
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
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result is -errno
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_fpc_written")); // keep the byte count of a successful write
    emitter.instruction("mov x0, #-1");                                         // a failed write() is file_put_contents()'s false, not an errno byte count
    emitter.label("__rt_fpc_written");
    // A SHORT write (RLIMIT_FSIZE, a full disk) is a failure too: php-src compares the byte
    // count with the payload length and answers false, never the partial count.
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload data length
    emitter.instruction("cmp x0, x9");                                          // was every byte written?
    emitter.instruction("mov x10, #-1");                                        // file_put_contents()'s failure result
    emitter.instruction("csel x0, x0, x10, eq");                                // keep the count only for a complete write
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

    emitter.instruction("b __rt_fpc_return");                                   // skip the open-failure exit

    // -- open() failed: nothing was opened, so nothing is closed or written --
    emitter.label("__rt_fpc_open_failed");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the quiet flag
    emitter.instruction("cbnz x9, __rt_fpc_open_failed_ret");                   // internal callers emit their own warning
    abi::emit_symbol_address(emitter, "x1", "_diag_file_put_contents_failed_msg");
    emitter.instruction(&format!("mov x2, #{}", FILE_PUT_CONTENTS_FAILED_WARNING.len())); // pass the warning byte length to the diagnostic helper
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the file_put_contents() failure warning
    emitter.label("__rt_fpc_open_failed_ret");
    emitter.instruction("mov x0, #-1");                                         // file_put_contents()'s failure result

    emitter.label("__rt_fpc_return");
    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
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
    emitter.comment("--- runtime: file_put_contents (internal, no warning) ---");
    emitter.label_global("__rt_file_put_contents_quiet");
    emitter.instruction("xor r8d, r8d");                                        // internal callers write with no PHP flags
    emitter.instruction("mov r9d, 1");                                          // the caller reports its own open failure
    emitter.instruction("jmp __rt_fpc_body_linux_x86_64");                      // share the flagged body

    emitter.blank();
    emitter.comment("--- runtime: file_put_contents (with PHP $flags) ---");
    emitter.label_global("__rt_file_put_contents_flagged");
    emitter.instruction("xor r9d, r9d");                                        // PHP-visible calls warn when the open fails
    emitter.instruction("jmp __rt_fpc_body_linux_x86_64");                      // share the body (global target, safe under section GC)
    emitter.label_global("__rt_fpc_body_linux_x86_64");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file_put_contents uses stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved pointers and lengths
    emitter.instruction("sub rsp, 64");                                         // reserve aligned stack space for data, path, fd, flags, quiet, and byte-count temporaries
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save the quiet flag across the cstr call

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
    emitter.instruction("test eax, eax");                                       // libc open() returns -1 as a C int on failure
    emitter.instruction("js __rt_fpc_open_failed_linux_x86_64");                // never hand -1 to flock()/write()/close()
    emitter.instruction("movsxd rax, eax");                                     // widen the C int descriptor
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
    emitter.instruction("test rax, rax");                                       // libc write() returns -1 (ssize_t) on failure
    emitter.instruction("jns __rt_fpc_written_linux_x86_64");                   // keep the byte count of a successful write
    emitter.instruction("mov rax, -1");                                         // normalize any failed write to file_put_contents()'s false
    emitter.label("__rt_fpc_written_linux_x86_64");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // was every byte written? a short write is a failure in php-src
    emitter.instruction("je __rt_fpc_complete_linux_x86_64");                   // keep the count only for a complete write
    emitter.instruction("mov rax, -1");                                         // file_put_contents()'s failure result for a short write
    emitter.label("__rt_fpc_complete_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the number of written bytes for the final return value

    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the file descriptor as the first libc close() argument
    emitter.instruction("call close");                                          // close the destination file after the write completes

    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the number of bytes reported by libc write()
    emitter.instruction("jmp __rt_fpc_return_linux_x86_64");                    // skip the lock-failure exit

    emitter.label("__rt_fpc_lock_failed_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the file descriptor
    emitter.instruction("call close");                                          // release the descriptor the refused lock leaves open
    emitter.instruction("mov rax, -1");                                         // file_put_contents()'s failure result

    emitter.instruction("jmp __rt_fpc_return_linux_x86_64");                    // skip the open-failure exit

    emitter.label("__rt_fpc_open_failed_linux_x86_64");
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // internal callers emit their own warning
    emitter.instruction("jne __rt_fpc_open_failed_ret_linux_x86_64");           // skip the PHP-visible warning for them
    abi::emit_symbol_address(emitter, "rdi", "_diag_file_put_contents_failed_msg"); // pass the file_put_contents() warning text pointer to the diagnostic helper
    emitter.instruction(&format!("mov esi, {}", FILE_PUT_CONTENTS_FAILED_WARNING.len())); // pass the warning byte length to the diagnostic helper
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the file_put_contents() failure warning
    emitter.label("__rt_fpc_open_failed_ret_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // file_put_contents()'s failure result

    emitter.label("__rt_fpc_return_linux_x86_64");
    emitter.instruction("add rsp, 64");                                         // release the aligned stack locals used by file_put_contents
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller with the write byte count in rax
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
    /// A failed open() must never reach write(): on Darwin the svc leaves the errno in x0 with
    /// the carry set, and ENOENT (2) is stderr's descriptor, so the payload went to stderr
    /// (php-src bug75679.phpt). Every target tests the open result before anything else.
    #[test]
    fn test_every_target_tests_the_open_result_before_writing() {
        for (target, open_call, guard) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "svc #0x80", "b.cc __rt_fpc_opened"),
            (Target::new(Platform::Linux, Arch::AArch64), "svc #0", "b.ge __rt_fpc_opened"),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "call open",
                "js __rt_fpc_open_failed_linux_x86_64",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_file_put_contents(&mut emitter);
            let asm = emitter.output();
            let open_at = asm.find(open_call).expect("the writer opens the destination");
            let after_open = &asm[open_at..];
            let guard_at = after_open
                .find(guard)
                .unwrap_or_else(|| panic!("{:?}: no open-failure guard in:\n{}", target.arch, after_open));
            let first_lock = after_open.find("__rt_flock").unwrap_or(usize::MAX);
            assert!(guard_at < first_lock, "{:?}: the guard must precede the lock and the write", target.arch);
        }
    }
}
