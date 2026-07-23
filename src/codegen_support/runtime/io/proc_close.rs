//! Purpose:
//! Emits the `__rt_proc_close` runtime helper for every supported target.
//! Unix reaps by child PID; Windows waits on, queries, and closes its retained
//! process HANDLE.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`, and from `__rt_mixed_free_deep` as
//!   the kind-5 resource destructor arm.
//!
//! Key details:
//! - Unix reaps the child with `wait4(pid, &status, 0, 0)` and returns
//!   `(status >> 8) & 0xff`; a normal exit previously harvested by
//!   `proc_get_status` is returned from the status cache. Windows uses
//!   `WaitForSingleObject` and `GetExitCodeProcess`. Failures return `-1` as the
//!   PHP integer result.
//! - The lowerer (`lower_proc_close`) stamps a `-1` sentinel into the resource
//!   box via `apply_resource_release_sentinel` so the kind-5 destructor arm in
//!   `__rt_mixed_free_deep` later skips reaping. `proc_close` therefore owns the
//!   reap and the destructor never re-reaps.
//! - Raw syscalls are emitted directly (not via `emitter.syscall()`/`map_syscall`)
//!   so this helper stays self-contained and does not perturb the shared syscall
//!   table. Linux `svc` does not set flags, so an explicit `cmp` precedes every
//!   conditional branch on the AArch64 Linux path.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

/// Emits `__rt_proc_close`: reaps the child and returns its exit code.
///
/// Input ABI: AArch64 `x0` = process descriptor (child pid); x86_64 `rdi` = pid.
/// Output: the child exit code (`0..255`) on success, or `-1` on `wait4` failure.
///
/// Target dispatch: AArch64 (macOS + Linux) shares one emitter that branches on
/// `emitter.platform` for the syscall number/mechanism; Linux-x86_64 gets its own
/// System V AMD64 variant; Windows-x86_64 gets its own MSx64
/// `WaitForSingleObject`/`GetExitCodeProcess` variant.
pub fn emit_proc_close(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_proc_close_aarch64(emitter),
        Arch::X86_64 => {
            if emitter.target.platform == Platform::Linux {
                emit_proc_close_linux_x86_64(emitter);
            } else {
                emit_proc_close_win32_x86_64(emitter);
            }
        }
    }
}

/// Emits the AArch64 `__rt_proc_close` runtime (macOS and Linux).
///
/// Uses a 32-byte frame: saved `x29`/`x30` at `[sp, #16]` and the wait-status
/// word at `[sp, #0]`. Branches on `emitter.platform` for the `wait4` syscall
/// number and trap convention (macOS `mov x16, #7` + `svc #0x80`; Linux
/// `mov x8, #260` + `svc #0`). Darwin reports syscall failure in carry, while
/// Linux returns a signed negative errno, so the target-specific tests must
/// run immediately after `svc` before any instruction overwrites NZCV.
fn emit_proc_close_aarch64(emitter: &mut Emitter) {
    let is_macos = emitter.target.platform == Platform::MacOS;
    emitter.blank();
    emitter.comment("--- runtime: proc_close (C1b reap) ---");
    emitter.label_global("__rt_proc_close");

    // -- prologue: 32-byte frame for the saved link register and status word --
    emitter.instruction("sub sp, sp, #32");                                     // reserve the proc_close frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- close every associated pipe before waiting for the child --
    abi::emit_call_label(emitter, "__rt_proc_pipe_registry_close");
    abi::emit_call_label(emitter, "__rt_proc_status_cached_exit");
    emitter.instruction("str x1, [sp, #0]");                                    // retain cache flag across metadata teardown
    emitter.instruction("str x2, [sp, #8]");                                    // retain cached normal exit code across teardown
    abi::emit_call_label(emitter, "__rt_proc_status_unregister");
    emitter.instruction("ldr x9, [sp, #0]");                                    // did proc_get_status already reap a normal exit?
    emitter.instruction("cbz x9, __rt_proc_close_wait");                        // no cache still requires blocking wait4
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the cached normal exit code
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the proc_close frame
    emitter.instruction("ret");                                                 // never wait twice for a cached child
    emitter.label("__rt_proc_close_wait");

    // -- wait4(pid, &status, 0, 0): x0 already holds the pid --
    emitter.instruction("add x1, sp, #0");                                      // status word address
    emitter.instruction("mov x2, #0");                                          // no wait options
    emitter.instruction("mov x3, #0");                                          // no rusage
    if is_macos {
        emitter.instruction("mov x16, #7");                                     // macOS wait4 syscall number
        emitter.instruction("svc #0x80");                                       // macOS AArch64 trap
        emitter.instruction("b.cs __rt_proc_close_err");                        // Darwin reports wait4 failure through carry
    } else {
        emitter.instruction("mov x8, #260");                                    // Linux wait4 syscall number
        emitter.instruction("svc #0");                                          // Linux AArch64 trap
        emitter.instruction("cmp x0, #0");                                      // Linux wait4 returns a signed negative errno on failure
        emitter.instruction("b.lt __rt_proc_close_err");                        // bail out with -1 on a reap failure
    }

    // -- extract the exit code: (status >> 8) & 0xff --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the raw wait status
    emitter.instruction("lsr x0, x9, #8");                                      // shift the exit code into the low byte
    emitter.instruction("and x0, x0, #0xff");                                   // mask to the 0..255 exit-code range
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the child exit code

    emitter.label("__rt_proc_close_err");
    emitter.instruction("mov x0, #-1");                                         // report reap failure to the lowerer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits the Linux-x86_64 `__rt_proc_close` runtime (System V AMD64).
///
/// Uses an `rbp` frame with the wait-status word at `[rbp-8]`. `rdi` already
/// holds the pid on entry. `wait4` is syscall 61 with `r10` carrying the
/// `rusage` argument (fourth syscall arg); the kernel clobbers `rcx`/`r11`.
fn emit_proc_close_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc_close (C1b reap) ---");
    emitter.label_global("__rt_proc_close");

    // -- prologue: rbp frame with one status word --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve cached-result and wait-status slots (16-byte aligned)

    // -- close every associated pipe before waiting for the child --
    abi::emit_call_label(emitter, "__rt_proc_pipe_registry_close");
    emitter.instruction("mov rdi, rax");                                        // restore pid after the registry helper returned it
    abi::emit_call_label(emitter, "__rt_proc_status_cached_exit");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // retain cache flag across metadata teardown
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // retain cached normal exit code across metadata teardown
    emitter.instruction("mov rdi, rax");                                        // restore pid before releasing its metadata
    abi::emit_call_label(emitter, "__rt_proc_status_unregister");
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // did proc_get_status already reap a normal exit?
    emitter.instruction("je __rt_proc_close_wait_x86");                         // no cache still requires blocking wait4
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the cached normal exit code
    emitter.instruction("add rsp, 32");                                         // release cached-result slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // never wait twice for a cached child
    emitter.label("__rt_proc_close_wait_x86");
    emitter.instruction("mov rdi, rax");                                        // restore pid after status metadata teardown

    // -- wait4(pid, &status, 0, 0): rdi already holds the pid --
    emitter.instruction("lea rsi, [rbp - 8]");                                  // status word address
    emitter.instruction("xor edx, edx");                                        // no wait options
    emitter.instruction("xor r10d, r10d");                                      // no rusage (fourth syscall arg via r10)
    emitter.instruction("mov eax, 61");                                         // Linux x86_64 wait4 syscall number
    emitter.instruction("syscall");                                             // Linux x86_64 trap (clobbers rcx/r11)
    emitter.instruction("test rax, rax");                                       // wait4 retval < 0 means reap failure
    emitter.instruction("js __rt_proc_close_err_x86");                          // bail out with -1 on a reap failure

    // -- extract the exit code: (status >> 8) & 0xff --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the raw wait status
    emitter.instruction("shr r9, 8");                                           // shift the exit code into the low byte
    emitter.instruction("movzx eax, r9b");                                      // mask to the 0..255 exit-code range and return
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the child exit code

    emitter.label("__rt_proc_close_err_x86");
    emitter.instruction("mov rax, -1");                                         // report reap failure to the lowerer
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits the Windows-x86_64 `__rt_proc_close` runtime (real C1c reap: MSx64
/// `WaitForSingleObject` + `GetExitCodeProcess` + `CloseHandle`).
///
/// Input: `rdi` = the process resource, an `hProcess` HANDLE (as returned by
/// `__rt_proc_open`'s `CreateProcessW`). Output: the child exit code
/// (`0..255`) on success, or `-1` on `WaitForSingleObject`/`GetExitCodeProcess`
/// failure. `rbp` frame: `[rbp - 8]` saves `hProcess` (`rcx`/`rdx` are
/// volatile and get clobbered between the three Win32 calls), `[rbp - 16]`
/// holds the DWORD exit code out-param for `GetExitCodeProcess`. 48 bytes
/// total (32-byte shadow space below `rsp` + the two 8-byte locals), 16-byte
/// aligned.
fn emit_proc_close_win32_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: proc_close (C1c reap) ---");
    emitter.label_global("__rt_proc_close");

    // -- prologue: rbp frame, 48 bytes (shadow(32) + hProcess(8) + exit_code(8)) --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the proc_close frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save hProcess (rcx/rdx clobbered by calls)

    // -- close every associated pipe before waiting for the child --
    abi::emit_call_label(emitter, "__rt_proc_pipe_registry_close");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // refresh hProcess after the registry helper returned it
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the preserved HANDLE to status-metadata teardown
    abi::emit_call_label(emitter, "__rt_proc_status_unregister");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // release retained status metadata before consuming the HANDLE

    // -- WaitForSingleObject(hProcess, INFINITE): rdi already holds hProcess --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // hProcess
    emitter.instruction("mov edx, 0xFFFFFFFF");                                 // dwMilliseconds = INFINITE
    emitter.instruction("call WaitForSingleObject");                            // block until the child process exits
    emitter.instruction("cmp eax, 0xFFFFFFFF");                                 // WAIT_FAILED?
    emitter.instruction("je __rt_proc_close_err_win");                          // reap failure -> report -1

    // -- GetExitCodeProcess(hProcess, &exit_code) --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload hProcess
    emitter.instruction("lea rdx, [rbp - 16]");                                 // &exit_code out-param
    emitter.instruction("call GetExitCodeProcess");                             // fetch the child exit code
    emitter.instruction("test eax, eax");                                       // GetExitCodeProcess failed?
    emitter.instruction("jz __rt_proc_close_err_win");                          // reap failure -> report -1

    // -- CloseHandle(hProcess): the resource is fully consumed after this reap --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload hProcess
    emitter.instruction("call CloseHandle");                                    // release the process handle
    emitter.instruction("mov eax, DWORD PTR [rbp - 16]");                       // reload the child exit code (0..255)
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the child exit code

    emitter.label("__rt_proc_close_err_win");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the consumed process HANDLE on every failure path
    emitter.instruction("call CloseHandle");                                    // release the HANDLE even though reap status retrieval failed
    emitter.instruction("mov rax, -1");                                         // report reap failure to the lowerer
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}
