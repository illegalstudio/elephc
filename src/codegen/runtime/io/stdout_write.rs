//! Purpose:
//! Emits the `__rt_stdout_write` runtime helper: the single indirection every
//! terminal stdout write travels through. Keeps the plain `write(1, …)` syscall
//! and the optional `--web` output-capture branch in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Calling convention (matches the C ABI of `elephc_web_write`): byte pointer
//!   in `x0`/`rdi`, length in `x1`/`rsi`. No return value.
//! - The `--web` capture branch (flag load + `elephc_web_write` call) is emitted
//!   ONLY when `web == true`. Non-web binaries never reference `_elephc_web_capture`
//!   or `elephc_web_write`, so they link without the (web-only) bridge symbol.
//! - The capture branch calls a C function, so a minimal frame is set up on every
//!   path: save/restore `x29`/`x30` (AArch64) and keep `rsp` 16-byte aligned across
//!   the `call` (x86_64), then `ret`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_stdout_write` runtime helper.
///
/// Inputs: byte pointer in `x0`/`rdi`, length in `x1`/`rsi`. No result.
///
/// When `web` is false (the universal default), unconditionally performs the
/// platform `write(1, ptr, len)` syscall. When `web` is true, first loads the
/// `_elephc_web_capture` flag: a zero flag takes the same syscall path, while a
/// non-zero flag tail-calls `elephc_web_write(ptr, len)` so the `--web` bridge can
/// capture the per-request response body.
///
/// Dispatches to `emit_stdout_write_x86_64` on x86_64; uses the AArch64 path
/// (covering macos-aarch64 and linux-aarch64) otherwise.
pub fn emit_stdout_write(emitter: &mut Emitter, web: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stdout_write_x86_64(emitter, web);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stdout_write ---");
    emitter.label_global("__rt_stdout_write");

    // -- set up a minimal frame so the capture branch can call a C function --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address (the capture branch clobbers x30)
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the call

    if web {
        // -- web build: route through elephc_web_write when capture is enabled --
        let capture_symbol = emitter.target.extern_symbol("elephc_web_capture");
        crate::codegen::abi::emit_symbol_address(emitter, "x9", &capture_symbol);
        emitter.instruction("ldrb w9, [x9]");                                   // load the low byte of the output-capture flag
        emitter.instruction("cbz x9, __rt_stdout_write_syscall");               // capture disabled — fall through to the plain write syscall
        emitter.bl_c("elephc_web_write");                                       // capture enabled — append the bytes to the current request's response body (ptr=x0, len=x1)
        emitter.instruction("b __rt_stdout_write_done");                        // capture handled the bytes — skip the syscall path
    }

    // -- plain write(1, ptr, len) syscall path --
    emitter.label("__rt_stdout_write_syscall");
    emitter.instruction("mov x2, x1");                                          // syscall len = incoming length (move before x1 is overwritten)
    emitter.instruction("mov x1, x0");                                          // syscall buf = incoming byte pointer
    emitter.instruction("mov x0, #1");                                          // syscall fd = stdout
    emitter.syscall(4);

    emitter.label("__rt_stdout_write_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of the `__rt_stdout_write` runtime helper.
///
/// Inputs: byte pointer in `rdi`, length in `rsi`. No result. Establishes an
/// `rbp` frame (which leaves `rsp` 16-byte aligned for the capture branch's
/// `call`), then either tail-calls `elephc_web_write` or performs the Linux
/// `write` syscall (`rax=1`, `rdi=fd`, `rsi=buf`, `rdx=len`).
fn emit_stdout_write_x86_64(emitter: &mut Emitter, web: bool) {
    emitter.blank();
    emitter.comment("--- runtime: stdout_write ---");
    emitter.label_global("__rt_stdout_write");

    // -- set up a minimal frame; after `push rbp` rsp is 16-byte aligned for the call --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align rsp for the capture-branch call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base

    if web {
        // -- web build: route through elephc_web_write when capture is enabled --
        let capture_symbol = emitter.target.extern_symbol("elephc_web_capture");
        crate::codegen::abi::emit_symbol_address(emitter, "r11", &capture_symbol);
        emitter.instruction("movzx r11d, BYTE PTR [r11]");                      // load the low byte of the output-capture flag, zero-extended
        emitter.instruction("test r11d, r11d");                                 // is per-request output capture enabled?
        emitter.instruction("jz __rt_stdout_write_syscall");                    // capture disabled — fall through to the plain write syscall
        emitter.bl_c("elephc_web_write");                                       // capture enabled — append the bytes to the current request's response body (ptr=rdi, len=rsi)
        emitter.instruction("jmp __rt_stdout_write_done");                      // capture handled the bytes — skip the syscall path
    }

    // -- plain write(1, ptr, len) syscall path --
    emitter.label("__rt_stdout_write_syscall");
    emitter.instruction("mov rdx, rsi");                                        // syscall len = incoming length (move before rsi is overwritten)
    emitter.instruction("mov rsi, rdi");                                        // syscall buf = incoming byte pointer
    emitter.instruction("mov edi, 1");                                          // syscall fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the bytes to stdout

    emitter.label("__rt_stdout_write_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// Renders the `__rt_stdout_write` helper for one target and web mode.
    fn render(platform: Platform, arch: Arch, web: bool) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_stdout_write(&mut emitter, web);
        emitter.output()
    }

    /// Verifies the helper always exports the global label every echo path calls.
    #[test]
    fn emits_global_label_for_all_targets() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            for web in [false, true] {
                let asm = render(platform, arch, web);
                assert!(
                    asm.contains(".globl __rt_stdout_write\n"),
                    "missing global label for {:?}/{:?} web={}",
                    platform,
                    arch,
                    web
                );
            }
        }
    }

    /// HARD GATE: non-web emission must never name the web-only bridge symbol or the
    /// capture flag, so non-web binaries link without the (web-only) `elephc_web_write`.
    #[test]
    fn non_web_never_references_web_symbols() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch, false);
            assert!(
                !asm.contains("elephc_web_write"),
                "non-web {:?}/{:?} must not reference elephc_web_write",
                platform,
                arch
            );
            assert!(
                !asm.contains("elephc_web_capture"),
                "non-web {:?}/{:?} must not reference the capture flag",
                platform,
                arch
            );
        }
    }

    /// Verifies web emission loads the capture flag and calls the bridge symbol,
    /// with the platform C-ABI mangling applied via `extern_symbol`/`bl_c`. The
    /// capture flag and the `elephc_web_write` symbol both carry the leading
    /// underscore on macOS and drop it on Linux, so the runtime references match
    /// the bridge's `extern "C"` declarations on every target.
    #[test]
    fn web_references_capture_flag_and_bridge() {
        let mac = render(Platform::MacOS, Arch::AArch64, true);
        assert!(mac.contains("_elephc_web_capture"));
        assert!(mac.contains("bl _elephc_web_write"));

        let linux_arm = render(Platform::Linux, Arch::AArch64, true);
        assert!(linux_arm.contains("elephc_web_capture"));
        assert!(!linux_arm.contains("_elephc_web_capture"));
        assert!(linux_arm.contains("bl elephc_web_write"));

        let linux_x86 = render(Platform::Linux, Arch::X86_64, true);
        assert!(linux_x86.contains("elephc_web_capture"));
        assert!(!linux_x86.contains("_elephc_web_capture"));
        assert!(linux_x86.contains("call elephc_web_write"));
    }

    /// Verifies both web and non-web emissions keep the plain `write(1, …)` syscall
    /// path, since the capture flag defaults to 0 and must fall through to the syscall.
    #[test]
    fn always_keeps_the_write_syscall_path() {
        // AArch64 (macOS): fd loaded into x0, then the macOS write trap.
        let mac = render(Platform::MacOS, Arch::AArch64, true);
        assert!(mac.contains("mov x0, #1"));
        assert!(mac.contains("mov x16, #4"));
        assert!(mac.contains("svc #0x80"));
        // x86_64 (Linux): syscall number 1 (write) in eax.
        let linux_x86 = render(Platform::Linux, Arch::X86_64, false);
        assert!(linux_x86.contains("mov eax, 1"));
        assert!(linux_x86.contains("syscall"));
    }
}
