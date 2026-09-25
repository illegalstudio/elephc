//! Purpose:
//! Emits elephc's own `__rt_setjmp` / `__rt_longjmp` runtime primitives for the
//! Windows x86_64 (PE32+) target — a plain callee-saved-register save/restore with
//! NO structured-exception-handling (SEH) involvement.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` on Windows x86_64 only.
//!
//! Key details:
//! - MinGW's C-library `setjmp`/`longjmp` on x86_64 Windows are SEH-based: `longjmp`
//!   unwinds via `RtlUnwindEx` and `setjmp` walks the stack to capture the SEH
//!   establisher frame. That walk runs off the top of elephc's small hand-rolled
//!   Fiber stacks (write access-violation at `stack_top`), and the MinGW entry points
//!   read their arguments in the MSx64 registers (`rcx`/`rdx`) while elephc's runtime
//!   passes them SysV-style (`rdi`/`rsi`). Both problems break every `try`/`catch`
//!   and every Fiber/Generator on Windows.
//! - elephc's exception model does not need Windows SEH: `setjmp` marks a resume
//!   point and `__rt_throw_current` `longjmp`s back to it after running its own frame
//!   cleanup. A plain register save/restore is therefore both sufficient and correct,
//!   and it works on any stack (main thread or Fiber) because it never walks frames or
//!   touches the TEB. These primitives take their arguments SysV-style (`rdi` =
//!   `jmp_buf`, `rsi` = value) exactly as elephc's `bl_c("setjmp"/"longjmp")` call
//!   sites already stage them, so no per-site ABI shuffle is required — `bl_c` simply
//!   routes `setjmp`/`longjmp` to these labels on the Windows x86_64 target.

use crate::codegen_support::emit::Emitter;

/// Emits `__rt_setjmp` and `__rt_longjmp`, elephc's SEH-free setjmp/longjmp pair for
/// Windows x86_64. Only emitted on the Windows x86_64 target; on every other target
/// the C-library `setjmp`/`longjmp` are used unchanged.
///
/// `__rt_setjmp` takes `rdi` = `jmp_buf` pointer and returns 0 in `eax`. It saves the
/// callee-saved registers, the caller's stack pointer, and the return address into the
/// 64-byte `jmp_buf` so a later `__rt_longjmp` can resume exactly where `__rt_setjmp`
/// returned.
///
/// `__rt_longjmp` takes `rdi` = `jmp_buf` pointer and `rsi` = return value. It restores
/// the saved registers and stack pointer, then jumps to the saved return address with
/// `eax` = the requested value (normalised to 1 when the caller passed 0, matching C
/// `longjmp` semantics).
///
/// jmp_buf layout (64 bytes, fits inside the 200-byte handler jmp_buf slot):
/// `+0` rbx, `+8` rbp, `+16` r12, `+24` r13, `+32` r14, `+40` r15, `+48` caller rsp,
/// `+56` return address.
pub fn emit_setjmp_longjmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: setjmp (SEH-free, Windows x86_64) ---");
    emitter.label_global("__rt_setjmp");

    emitter.instruction("mov QWORD PTR [rdi], rbx");                            // jmp_buf+0  = rbx (callee-saved)
    emitter.instruction("mov QWORD PTR [rdi + 8], rbp");                        // jmp_buf+8  = rbp (frame pointer)
    emitter.instruction("mov QWORD PTR [rdi + 16], r12");                       // jmp_buf+16 = r12 (callee-saved)
    emitter.instruction("mov QWORD PTR [rdi + 24], r13");                       // jmp_buf+24 = r13 (callee-saved)
    emitter.instruction("mov QWORD PTR [rdi + 32], r14");                       // jmp_buf+32 = r14 (callee-saved)
    emitter.instruction("mov QWORD PTR [rdi + 40], r15");                       // jmp_buf+40 = r15 (callee-saved)
    emitter.instruction("lea rax, [rsp + 8]");                                  // rax = caller's rsp (past the pushed return address)
    emitter.instruction("mov QWORD PTR [rdi + 48], rax");                       // jmp_buf+48 = caller stack pointer
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // rax = return address left by the call into __rt_setjmp
    emitter.instruction("mov QWORD PTR [rdi + 56], rax");                       // jmp_buf+56 = resume address
    emitter.instruction("xor eax, eax");                                        // setjmp returns 0 on the direct path
    emitter.instruction("ret");                                                 // resume the caller with a zero result

    emitter.blank();
    emitter.comment("--- runtime: longjmp (SEH-free, Windows x86_64) ---");
    emitter.label_global("__rt_longjmp");

    emitter.instruction("mov rbx, QWORD PTR [rdi]");                            // restore rbx from jmp_buf+0
    emitter.instruction("mov rbp, QWORD PTR [rdi + 8]");                        // restore rbp from jmp_buf+8
    emitter.instruction("mov r12, QWORD PTR [rdi + 16]");                       // restore r12 from jmp_buf+16
    emitter.instruction("mov r13, QWORD PTR [rdi + 24]");                       // restore r13 from jmp_buf+24
    emitter.instruction("mov r14, QWORD PTR [rdi + 32]");                       // restore r14 from jmp_buf+32
    emitter.instruction("mov r15, QWORD PTR [rdi + 40]");                       // restore r15 from jmp_buf+40
    emitter.instruction("mov rdx, QWORD PTR [rdi + 56]");                       // rdx = saved resume address (before rsp is switched)
    emitter.instruction("mov rsp, QWORD PTR [rdi + 48]");                       // adopt the saved caller stack pointer
    emitter.instruction("mov eax, esi");                                        // eax = requested longjmp return value
    emitter.instruction("test eax, eax");                                       // was the requested value zero?
    emitter.instruction("jne __rt_longjmp_resume");                             // non-zero values pass through unchanged
    emitter.instruction("mov eax, 1");                                          // C longjmp normalises a zero value to 1
    emitter.label("__rt_longjmp_resume");
    emitter.instruction("jmp rdx");                                             // resume execution at the saved __rt_setjmp return point
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies `__rt_setjmp`/`__rt_longjmp` are emitted with the plain register
    /// save/restore body and the SysV `rdi` jmp_buf pointer convention.
    #[test]
    fn test_emits_plain_setjmp_longjmp() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_setjmp_longjmp(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_setjmp"), "missing __rt_setjmp label");
        assert!(asm.contains("__rt_longjmp"), "missing __rt_longjmp label");
        assert!(asm.contains("mov QWORD PTR [rdi], rbx"));
        assert!(asm.contains("mov QWORD PTR [rdi + 48], rax"));
        assert!(asm.contains("jmp rdx"));
        assert!(!asm.contains("call"), "the SEH-free primitives must be leaf helpers");
    }

    /// Verifies `bl_c` routes `setjmp`/`longjmp` to the SEH-free helpers on the
    /// Windows x86_64 target while leaving the C-library names untouched elsewhere,
    /// so non-Windows codegen stays byte-identical.
    #[test]
    fn test_bl_c_routes_setjmp_longjmp_on_windows_only() {
        let mut win = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        win.bl_c("setjmp");
        win.bl_c("longjmp");
        let win_asm = win.output();
        assert!(win_asm.contains("call __rt_setjmp"));
        assert!(win_asm.contains("call __rt_longjmp"));

        let mut linux = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        linux.bl_c("setjmp");
        linux.bl_c("longjmp");
        let linux_asm = linux.output();
        assert!(linux_asm.contains("call setjmp"));
        assert!(linux_asm.contains("call longjmp"));
        assert!(!linux_asm.contains("__rt_setjmp"));
        assert!(!linux_asm.contains("__rt_longjmp"));
    }
}
