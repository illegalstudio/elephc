//! Purpose:
//! Emits the shared x86_64 static-message Throwable allocator used by runtime error paths.
//! Keeps alternative throw branches out of their caller's unwind range.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Inputs use the internal SysV convention: `rdi` class id, `rsi` message pointer, `rdx` length.
//! - The helper never returns; it publishes `_exc_value` and tail-calls `__rt_throw_current`.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the shared x86_64 static-message Throwable allocation helper.
pub fn emit_throw_static_exception(emitter: &mut Emitter) {
    if emitter.target.arch != Arch::X86_64 {
        return;
    }
    emitter.label_global("__rt_throw_static_exception");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while allocating
    emitter.instruction("mov rbp, rsp");                                        // establish the shared throw-helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve aligned class, message, and length spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the exception class id
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the static message pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the static message length
    emitter.instruction("mov rax, 56");                                         // request Throwable payload storage (message/code/previous)
    emitter.instruction("call __rt_heap_alloc");                                // allocate the exception object payload
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // stamp the canonical x86_64 throwable heap kind
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the allocation kind before unwinding
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the exception class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id in the object header
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the static message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the exception message pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the static message length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the exception message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // default the exception code to zero
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // default the previous Throwable to null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the active Throwable
    emitter.instruction("mov rsp, rbp");                                        // release the shared throw-helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
    emitter.blank();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies the shared x86_64 throw helper uses the canonical object header and full layout.
    #[test]
    fn x86_64_static_throw_uses_canonical_throwable_layout() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_throw_static_exception(&mut emitter);
        let asm = emitter.output();
        let kind = crate::codegen_support::sentinels::x86_64_heap_kind_word(6);
        assert!(asm.contains("mov rax, 56"));
        assert!(asm.contains(&format!("mov r10, 0x{kind:x}")));
        assert!(asm.contains("mov QWORD PTR [rax + 40], 0"));
        assert!(asm.contains("jmp __rt_throw_current"));
    }

    /// Verifies ARM64 keeps using its target-specific in-place throw paths.
    #[test]
    fn aarch64_does_not_emit_x86_64_static_throw_helper() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_throw_static_exception(&mut emitter);
        assert!(emitter.output().is_empty());
    }
}
