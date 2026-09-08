//! Purpose:
//! Dispatches generated private DateTime-subclass hydrators after native ext/date state restore.
//!
//! Called from:
//! - `__elephc_restore_date_properties()` marker lowering inside the five native date classes.
//!
//! Key details:
//! - Class-id descriptors invoke AST-compiled helpers in declaring-class order, never raw-copying
//!   parsed cells into typed slots and never exposing an overrideable PHP method.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the target-specific dispatcher returning the helpers' filtered data hash.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_filter_references(emitter);
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the target-specific pre-hydration filter for serialized date references.
fn emit_filter_references(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_filter_references_aarch64(emitter),
        Arch::X86_64 => emit_filter_references_x86_64(emitter),
    }
}

/// Emits the AArch64 `(object, data) -> filtered_data` reference filter.
fn emit_filter_references_aarch64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: date_magic_filter_refs ---");
    emitter.label_global("__rt_date_magic_filter_refs");
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("mov x0, x1");                                        // pass the `$data` hash to the copy-on-write boundary
    emitter.instruction("bl __rt_hash_ensure_unique");                         // stabilize slot indexes before the iterator emits a cursor
    emitter.instruction("str x0, [sp, #0]");                                  // retain the unique `$data` hash while references are removed
    emitter.instruction("str xzr, [sp, #8]");                                 // initialize the hash iterator cursor
    emitter.label("__rt_date_magic_filter_refs_loop");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("bl __rt_hash_iter_next");
    emitter.instruction("cmn x0, #1");
    emitter.instruction("b.eq __rt_date_magic_filter_refs_done");
    emitter.instruction("str x0, [sp, #8]");
    emitter.instruction("str x1, [sp, #16]");
    emitter.instruction("str x2, [sp, #24]");
    emitter.instruction("cmp x5, #7");                                        // only boxed Mixed entries carry serialized reference provenance
    emitter.instruction("b.ne __rt_date_magic_filter_refs_loop");
    emitter.instruction("cbnz x4, __rt_date_magic_filter_refs_remove");       // decoder retained an `R:` provenance marker
    emitter.instruction("cbz x3, __rt_date_magic_filter_refs_loop");
    emitter.instruction("ldr x9, [x3]");
    emitter.instruction("cmp x9, #11");                                       // also discard an explicit reference box
    emitter.instruction("b.ne __rt_date_magic_filter_refs_loop");
    emitter.label("__rt_date_magic_filter_refs_remove");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x1, [sp, #16]");
    emitter.instruction("ldr x2, [sp, #24]");
    emitter.instruction("bl __rt_hash_unset");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("b __rt_date_magic_filter_refs_loop");
    emitter.label("__rt_date_magic_filter_refs_done");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// Emits the x86_64 `(object, data) -> filtered_data` reference filter.
fn emit_filter_references_x86_64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: date_magic_filter_refs ---");
    emitter.label_global("__rt_date_magic_filter_refs");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 64");
    emitter.instruction("mov rdi, rsi");                                      // pass the `$data` hash to the copy-on-write boundary
    emitter.instruction("call __rt_hash_ensure_unique");                       // stabilize slot indexes before the iterator emits a cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                      // retain the unique `$data` hash while references are removed
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                       // initialize the hash iterator cursor
    emitter.label("__rt_date_magic_filter_refs_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_hash_iter_next");
    emitter.instruction("cmp rax, -1");
    emitter.instruction("je __rt_date_magic_filter_refs_done");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");
    emitter.instruction("cmp r9, 7");                                         // only boxed Mixed entries carry serialized reference provenance
    emitter.instruction("jne __rt_date_magic_filter_refs_loop");
    emitter.instruction("test r8, r8");
    emitter.instruction("jnz __rt_date_magic_filter_refs_remove");            // decoder retained an `R:` provenance marker
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_date_magic_filter_refs_loop");
    emitter.instruction("cmp QWORD PTR [rcx], 11");                            // also discard an explicit reference box
    emitter.instruction("jne __rt_date_magic_filter_refs_loop");
    emitter.label("__rt_date_magic_filter_refs_remove");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    emitter.instruction("call __rt_hash_unset");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");
    emitter.instruction("jmp __rt_date_magic_filter_refs_loop");
    emitter.label("__rt_date_magic_filter_refs_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("add rsp, 64");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits the AArch64 dispatcher for `(object, data) -> filtered_data`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: date_magic_restore_props ---");
    emitter.label_global("__rt_date_magic_restore_props");
    emitter.instruction("sub sp, sp, #96");
    emitter.instruction("stp x29, x30, [sp, #80]");
    emitter.instruction("add x29, sp, #80");
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("ldr x9, [x0]");
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "x10",
        "_class_date_restore_helper_ptrs",
    );
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");
    emitter.instruction("str x10, [sp, #16]");
    emitter.instruction("ldr x11, [x10]");
    emitter.instruction("str x11, [sp, #24]");
    emitter.instruction("str xzr, [sp, #32]");
    emitter.instruction("str xzr, [sp, #40]");
    emitter.label("__rt_date_magic_restore_refs_loop");
    emitter.instruction("ldr x0, [sp, #8]");
    emitter.instruction("ldr x1, [sp, #40]");
    emitter.instruction("bl __rt_hash_iter_next");
    emitter.instruction("cmn x0, #1");
    emitter.instruction("b.eq __rt_date_magic_restore_refs_done");
    emitter.instruction("str x0, [sp, #40]");
    emitter.instruction("str x1, [sp, #48]");
    emitter.instruction("str x2, [sp, #56]");
    emitter.instruction("cmp x5, #7");
    emitter.instruction("b.ne __rt_date_magic_restore_refs_loop");
    emitter.instruction("cbnz x4, __rt_date_magic_restore_refs_remove");
    emitter.instruction("cbz x3, __rt_date_magic_restore_refs_loop");
    emitter.instruction("ldr x9, [x3]");
    emitter.instruction("cmp x9, #11");
    emitter.instruction("b.ne __rt_date_magic_restore_refs_loop");
    emitter.label("__rt_date_magic_restore_refs_remove");
    emitter.instruction("ldr x0, [sp, #8]");
    emitter.instruction("ldr x1, [sp, #48]");
    emitter.instruction("ldr x2, [sp, #56]");
    emitter.instruction("bl __rt_hash_unset");
    emitter.instruction("str x0, [sp, #8]");
    emitter.instruction("b __rt_date_magic_restore_refs_loop");
    emitter.label("__rt_date_magic_restore_refs_done");
    emitter.label("__rt_date_magic_restore_props_loop");
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("ldr x10, [sp, #24]");
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ge __rt_date_magic_restore_props_done");
    emitter.instruction("ldr x10, [sp, #16]");
    emitter.instruction("add x10, x10, #8");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("blr x11");
    emitter.instruction("str x0, [sp, #8]");
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #32]");
    emitter.instruction("b __rt_date_magic_restore_props_loop");
    emitter.label("__rt_date_magic_restore_props_done");
    emitter.instruction("ldr x0, [sp, #8]");
    emitter.instruction("ldp x29, x30, [sp, #80]");
    emitter.instruction("add sp, sp, #96");
    emitter.instruction("ret");
}

/// Emits the x86_64 dispatcher for `(object, data) -> filtered_data`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: date_magic_restore_props ---");
    emitter.label_global("__rt_date_magic_restore_props");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 80");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    emitter.instruction("mov rax, QWORD PTR [rdi]");
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "r10",
        "_class_date_restore_helper_ptrs",
    );
    emitter.instruction("mov r10, QWORD PTR [r10 + rax*8]");
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");
    emitter.instruction("mov r11, QWORD PTR [r10]");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");
    emitter.label("__rt_date_magic_restore_refs_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");
    emitter.instruction("call __rt_hash_iter_next");
    emitter.instruction("cmp rax, -1");
    emitter.instruction("je __rt_date_magic_restore_refs_done");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");
    emitter.instruction("cmp r9, 7");
    emitter.instruction("jne __rt_date_magic_restore_refs_loop");
    emitter.instruction("test r8, r8");
    emitter.instruction("jnz __rt_date_magic_restore_refs_remove");
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_date_magic_restore_refs_loop");
    emitter.instruction("cmp QWORD PTR [rcx], 11");
    emitter.instruction("jne __rt_date_magic_restore_refs_loop");
    emitter.label("__rt_date_magic_restore_refs_remove");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");
    emitter.instruction("call __rt_hash_unset");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");
    emitter.instruction("jmp __rt_date_magic_restore_refs_loop");
    emitter.label("__rt_date_magic_restore_refs_done");
    emitter.label("__rt_date_magic_restore_props_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("cmp rax, QWORD PTR [rbp - 32]");
    emitter.instruction("jge __rt_date_magic_restore_props_done");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction("mov r11, QWORD PTR [r10 + rax*8 + 8]");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call r11");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");
    emitter.instruction("add QWORD PTR [rbp - 40], 1");
    emitter.instruction("jmp __rt_date_magic_restore_props_loop");
    emitter.label("__rt_date_magic_restore_props_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
    emitter.instruction("add rsp, 80");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies COW completes before the AArch64 filter records an insertion-order cursor.
    #[test]
    fn filter_references_stabilizes_aarch64_hash_before_iteration() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_filter_references(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains(
            "mov x0, x1\n    bl __rt_hash_ensure_unique\n    str x0, [sp, #0]\n    str xzr, [sp, #8]"
        ));
    }

    /// Verifies COW completes before the x86_64 filter records an insertion-order cursor.
    #[test]
    fn filter_references_stabilizes_x86_hash_before_iteration() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_filter_references(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains(
            "mov rdi, rsi\n    call __rt_hash_ensure_unique\n    mov QWORD PTR [rbp - 8], rax\n    mov QWORD PTR [rbp - 16], 0"
        ));
    }
}
