//! Purpose:
//! Emits the `__rt_concat`, `__rt_concat_cl` runtime helper assembly for runtime string concatenation.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - Destination storage comes from `__rt_concat_reserve`, so a result that no longer fits the
//!   heap-sized `_concat_buf` result arena lands in an owned heap block instead of running
//!   past the buffer end into the adjacent BSS globals.
//! - A heap-backed result is stamped with `CONCAT_TEMP_HEAP_KIND` so `__rt_str_persist` can
//!   take it over in place rather than allocating a second copy of it.
//! - `left_len + right_len` is checked for unsigned wrap before the reservation, so a wrapped
//!   total can never size a destination smaller than the bytes the copy loops write.

pub use elephc_memoir::strings::concat::emit_concat;

#[cfg(test)]
use crate::codegen_support::emit::Emitter;

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    #[test]
    /// Verifies that the x86_64 Linux concat path reserves bounded destination storage,
    /// uses native byte-copy loops, and returns the result pointer/length in rax/rdx per
    /// the AMD64 ABI convention.
    fn test_emit_concat_linux_x86_64_uses_native_copy_loop() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_concat(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_concat:\n"));
        assert!(asm.contains("mov QWORD PTR [rbp - 8], rax\n"));
        assert!(asm.contains("call __rt_concat_reserve\n"));
        assert!(asm.contains("call __rt_concat_publish\n"));
        assert!(asm.contains("mov r11b, BYTE PTR [r8]\n"));
        assert!(asm.contains("mov rax, QWORD PTR [rbp - 48]\n"));
        assert!(asm.contains(".p2align 6\n__rt_concat_cl:\n"));
    }

    #[test]
    /// Verifies that the AArch64 concat path bounds its destination through the shared
    /// reservation helpers and rejects a wrapped `left_len + right_len` total.
    fn test_emit_concat_aarch64_reserves_bounded_destination() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_concat(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("bl __rt_concat_reserve\n"));
        assert!(asm.contains("bl __rt_concat_publish\n"));
        assert!(asm.contains("adds x5, x2, x4\n"));
        assert!(asm.contains("b.cs __rt_concat_size_overflow\n"));
        assert!(asm.contains(".p2align 6\n__rt_concat_cl:\n"));
    }
}
