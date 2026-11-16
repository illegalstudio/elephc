//! Purpose:
//! Preserves compiler imports and tests for Memoir value storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::values::array_clone_shallow::*;


#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    #[test]
    /// Verifies the x86_64 clone helper's stack frame keeps locals (source pointer at
    /// `[rbp - 40]`, cloned pointer at `[rbp - 48]`) below the saved callee-saved
    /// registers, which sit at `[rbp - 8]` through `[rbp - 32]` after the `sub rsp, 32`.
    fn test_x86_clone_frame_keeps_locals_below_saved_callee_registers() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_array_clone_shallow(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("sub rsp, 32\n"));
        assert!(asm.contains("mov QWORD PTR [rbp - 40], rdi\n"));
        assert!(asm.contains("mov QWORD PTR [rbp - 48], rax\n"));
        assert!(!asm.contains("mov QWORD PTR [rbp - 8], rdi\n"));
        assert!(!asm.contains("mov QWORD PTR [rbp - 16], rax\n"));
    }

    #[test]
    /// Verifies the x86_64 clone helper recomputes the string slot address after
    /// `__rt_str_persist` clobbers caller-saved registers (r10, r11, r8, rax, rdx),
    /// confirming the loop reloads indices from spill slots rather than assuming
    /// they survive the helper call.
    fn test_x86_clone_string_fixup_recomputes_slot_after_persist() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_array_clone_shallow(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov QWORD PTR [rbp - 56], 0\n"));
        assert!(asm.contains("call __rt_str_persist\n    mov r10, QWORD PTR [rbp - 56]\n"));
        assert!(asm.contains("mov r8, QWORD PTR [rbp - 48]\n    lea r11, [r8 + r11 + 24]\n"));
        assert!(asm.contains("call __rt_incref\n__rt_array_clone_shallow_next_child:\n    mov r10, QWORD PTR [rbp - 56]\n"));
    }
}
