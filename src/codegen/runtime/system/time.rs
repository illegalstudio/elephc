//! Purpose:
//! Emits the `__rt_time` runtime helper assembly for time.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - System helpers must preserve PHP-visible behavior while crossing libc, syscall, JSON, regex, and date formatter boundaries.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Platform};

/// Emits the `__rt_time` runtime helper for the current platform.
/// Routes to `emit_time_macos_arm64`, `emit_time_linux_arm64`, or `emit_time_linux_x86_64`
/// depending on target. Output: x0 (ARM64) or rax (x86_64) = seconds since Unix epoch.
pub(crate) fn emit_time(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_time_linux_x86_64(emitter);
        return;
    }

    if emitter.platform == Platform::MacOS {
        emit_time_macos_arm64(emitter);
        return;
    }

    emit_time_linux_arm64(emitter);
}

/// Emits `__rt_time` for macOS ARM64. Routes through libc `time(NULL)` so that
/// libsystem's lazy TLS/errno init runs before any subsequent libc call (notably `localtime`).
/// Raw syscalls bypass that init path and reproducibly crash when `tzset` first reads `environ`.
/// Output: x0 = seconds since Unix epoch.
fn emit_time_macos_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: time ---");
    emitter.label_global("__rt_time");

    // -- set up minimal frame and call libc time(NULL) --
    // Going through libc rather than a raw `svc` ensures libsystem's TLS/__findenv state is
    // initialized before any later `localtime` chain runs. Raw syscalls bypass that init
    // path and reproducibly crash deeper inside libc when `tzset` first reads `environ`.
    emitter.instruction("sub sp, sp, #16");                                     // allocate frame (16 bytes, 16-aligned)
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // new frame pointer
    emitter.instruction("mov x0, #0");                                          // x0 = NULL (no out-param needed)
    emitter.bl_c("time");                                                       // libc time(NULL) → x0 = Unix timestamp
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // tear down frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_time` for Linux ARM64 using the raw `gettimeofday` syscall.
/// No comparable TLS init hazard on glibc, so a raw syscall is safe.
/// Output: x0 = seconds since Unix epoch.
fn emit_time_linux_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: time ---");
    emitter.label_global("__rt_time");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes (16 for timeval + 16 for frame + padding)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    // -- call gettimeofday syscall --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timeval struct on stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.syscall(116);

    // -- extract tv_sec from timeval struct --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = tv_sec (first 8 bytes of timeval)

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_time` for Linux x86_64 via libc `gettimeofday`.
/// Allocates a temporary timeval on the stack, passes it to `gettimeofday`, and returns tv_sec.
/// Output: rax = seconds since Unix epoch.
fn emit_time_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: time ---");
    emitter.label_global("__rt_time");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating the temporary timeval storage for libc gettimeofday()
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary timeval storage used by libc gettimeofday()
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack storage for one timeval struct plus scratch padding before the libc call
    emitter.instruction("lea rdi, [rsp]");                                      // pass the temporary timeval storage as the first SysV integer argument to libc gettimeofday()
    emitter.instruction("xor esi, esi");                                        // pass NULL as the timezone pointer because elephc only needs the current Unix timestamp
    emitter.bl_c("gettimeofday");                                               // fill the temporary timeval with the current wall-clock time through libc
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // return tv_sec from the temporary timeval as the current Unix timestamp in the native integer result register
    emitter.instruction("leave");                                               // release the temporary timeval storage and restore the caller frame pointer in one step
    emitter.instruction("ret");                                                 // return the current Unix timestamp to generated code
}
