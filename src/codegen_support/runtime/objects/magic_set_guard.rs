//! Purpose:
//! Emits the runtime reentrancy guard used by dynamic-name `__set` dispatch.
//!
//! Called from:
//! - Runtime-name object property writes before and after invoking `__set`.
//!
//! Key details:
//! - Guard identity is receiver identity plus case-sensitive property bytes.
//! - `_magic_set_guard_head` is switched with Fiber execution context, so it only refers to
//!   nodes on the current stack and cannot retain pointers into an unmapped Fiber stack.
//! - Nodes live in the generated caller's guarded stack frame and are always unlinked before
//!   that frame is released, including the local exception-boundary path.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits guard insertion and removal helpers for dynamic-name magic property writes.
pub(crate) fn emit_magic_set_guard(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: magic __set reentrancy guard ---");
    emitter.label_global("__rt_magic_set_guard_push");
    crate::codegen_support::abi::emit_load_symbol_to_reg(
        emitter,
        "x9",
        "_magic_set_guard_head",
        0,
    );
    emitter.instruction("mov x11, x9");                                         // scan the active guard chain from its head
    emitter.label("__rt_magic_set_guard_push_scan");
    emitter.instruction("cbz x11, __rt_magic_set_guard_push_new");              // no matching active receiver/name pair exists
    emitter.instruction("ldr x12, [x11, #8]");                                  // compare receiver identity
    emitter.instruction("cmp x12, x0");                                         // emit the guarded operation
    emitter.instruction("b.ne __rt_magic_set_guard_push_next");                 // emit the guarded operation
    emitter.instruction("ldr x12, [x11, #24]");                                 // compare property-name length
    emitter.instruction("cmp x12, x2");                                         // emit the guarded operation
    emitter.instruction("b.ne __rt_magic_set_guard_push_next");                 // emit the guarded operation
    emitter.instruction("ldr x12, [x11, #16]");                                 // x12 = active property bytes
    emitter.instruction("mov x13, #0");                                         // x13 = byte index
    emitter.label("__rt_magic_set_guard_push_bytes");
    emitter.instruction("cmp x13, x2");                                         // emit the guarded operation
    emitter.instruction("b.eq __rt_magic_set_guard_push_found");                // emit the guarded operation
    emitter.instruction("ldrb w14, [x12, x13]");                                // emit the guarded operation
    emitter.instruction("ldrb w15, [x1, x13]");                                 // emit the guarded operation
    emitter.instruction("cmp w14, w15");                                        // emit the guarded operation
    emitter.instruction("b.ne __rt_magic_set_guard_push_next");                 // emit the guarded operation
    emitter.instruction("add x13, x13, #1");                                    // emit the guarded operation
    emitter.instruction("b __rt_magic_set_guard_push_bytes");                   // emit the guarded operation
    emitter.label("__rt_magic_set_guard_push_next");
    emitter.instruction("ldr x11, [x11]");                                      // follow node.next
    emitter.instruction("b __rt_magic_set_guard_push_scan");                    // emit the guarded operation
    emitter.label("__rt_magic_set_guard_push_found");
    emitter.instruction("mov x0, #0");                                          // matching pair is already active, suppress __set
    emitter.instruction("ret");                                                 // emit the guarded operation
    emitter.label("__rt_magic_set_guard_push_new");
    emitter.instruction("str x9, [x3]");                                        // node.next = old head
    emitter.instruction("str x0, [x3, #8]");                                    // node.receiver = receiver
    emitter.instruction("str x1, [x3, #16]");                                   // node.name_ptr = property bytes
    emitter.instruction("str x2, [x3, #24]");                                   // node.name_len = property length
    crate::codegen_support::abi::emit_store_reg_to_symbol(
        emitter,
        "x3",
        "_magic_set_guard_head",
        0,
    );
    emitter.instruction("mov x0, #1");                                          // this caller owns the new guard node
    emitter.instruction("ret");                                                 // emit the guarded operation

    emitter.label_global("__rt_magic_set_guard_pop");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_magic_set_guard_head");
    emitter.instruction("ldr x10, [x9]");                                       // x9 is the link that points at the current node
    emitter.label("__rt_magic_set_guard_pop_scan");
    emitter.instruction("cbz x10, __rt_magic_set_guard_pop_done");              // tolerate a defensive unmatched pop
    emitter.instruction("cmp x10, x0");                                         // emit the guarded operation
    emitter.instruction("b.eq __rt_magic_set_guard_pop_found");                 // emit the guarded operation
    emitter.instruction("mov x9, x10");                                         // &node.next is the node address itself
    emitter.instruction("ldr x10, [x10]");                                      // emit the guarded operation
    emitter.instruction("b __rt_magic_set_guard_pop_scan");                     // emit the guarded operation
    emitter.label("__rt_magic_set_guard_pop_found");
    emitter.instruction("ldr x11, [x10]");                                      // unlink the exact caller-owned node
    emitter.instruction("str x11, [x9]");                                       // emit the guarded operation
    emitter.label("__rt_magic_set_guard_pop_done");
    emitter.instruction("ret");                                                 // emit the guarded operation
}

fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: magic __set reentrancy guard ---");
    emitter.label_global("__rt_magic_set_guard_push");
    crate::codegen_support::abi::emit_load_symbol_to_reg(
        emitter,
        "r8",
        "_magic_set_guard_head",
        0,
    );
    emitter.instruction("mov r10, r8");                                         // scan the active guard chain from its head
    emitter.label("__rt_magic_set_guard_push_scan");
    emitter.instruction("test r10, r10");                                       // emit the guarded operation
    emitter.instruction("jz __rt_magic_set_guard_push_new");                    // no matching active receiver/name pair exists
    emitter.instruction("cmp QWORD PTR [r10 + 8], rdi");                        // compare receiver identity
    emitter.instruction("jne __rt_magic_set_guard_push_next");                  // emit the guarded operation
    emitter.instruction("cmp QWORD PTR [r10 + 24], rdx");                       // compare property-name length
    emitter.instruction("jne __rt_magic_set_guard_push_next");                  // emit the guarded operation
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // r11 = active property bytes
    emitter.instruction("xor eax, eax");                                        // rax = byte index
    emitter.label("__rt_magic_set_guard_push_bytes");
    emitter.instruction("cmp rax, rdx");                                        // emit the guarded operation
    emitter.instruction("je __rt_magic_set_guard_push_found");                  // emit the guarded operation
    emitter.instruction("mov r8b, BYTE PTR [r11 + rax]");                       // emit the guarded operation
    emitter.instruction("cmp r8b, BYTE PTR [rsi + rax]");                       // emit the guarded operation
    emitter.instruction("jne __rt_magic_set_guard_push_next");                  // emit the guarded operation
    emitter.instruction("inc rax");                                             // emit the guarded operation
    emitter.instruction("jmp __rt_magic_set_guard_push_bytes");                 // emit the guarded operation
    emitter.label("__rt_magic_set_guard_push_next");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // follow node.next
    emitter.instruction("jmp __rt_magic_set_guard_push_scan");                  // emit the guarded operation
    emitter.label("__rt_magic_set_guard_push_found");
    emitter.instruction("xor eax, eax");                                        // matching pair is already active, suppress __set
    emitter.instruction("ret");                                                 // emit the guarded operation
    emitter.label("__rt_magic_set_guard_push_new");
    crate::codegen_support::abi::emit_load_symbol_to_reg(
        emitter,
        "r8",
        "_magic_set_guard_head",
        0,
    );
    emitter.instruction("mov QWORD PTR [rcx], r8");                             // node.next = old head
    emitter.instruction("mov QWORD PTR [rcx + 8], rdi");                        // node.receiver = receiver
    emitter.instruction("mov QWORD PTR [rcx + 16], rsi");                       // node.name_ptr = property bytes
    emitter.instruction("mov QWORD PTR [rcx + 24], rdx");                       // node.name_len = property length
    crate::codegen_support::abi::emit_store_reg_to_symbol(
        emitter,
        "rcx",
        "_magic_set_guard_head",
        0,
    );
    emitter.instruction("mov eax, 1");                                          // this caller owns the new guard node
    emitter.instruction("ret");                                                 // emit the guarded operation

    emitter.label_global("__rt_magic_set_guard_pop");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_magic_set_guard_head");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // r8 is the link that points at the current node
    emitter.label("__rt_magic_set_guard_pop_scan");
    emitter.instruction("test r9, r9");                                         // emit the guarded operation
    emitter.instruction("jz __rt_magic_set_guard_pop_done");                    // tolerate a defensive unmatched pop
    emitter.instruction("cmp r9, rdi");                                         // emit the guarded operation
    emitter.instruction("je __rt_magic_set_guard_pop_found");                   // emit the guarded operation
    emitter.instruction("mov r8, r9");                                          // &node.next is the node address itself
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // emit the guarded operation
    emitter.instruction("jmp __rt_magic_set_guard_pop_scan");                   // emit the guarded operation
    emitter.label("__rt_magic_set_guard_pop_found");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // unlink the exact caller-owned node
    emitter.instruction("mov QWORD PTR [r8], r10");                             // emit the guarded operation
    emitter.label("__rt_magic_set_guard_pop_done");
    emitter.instruction("ret");                                                 // emit the guarded operation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    #[test]
    fn emits_receiver_name_guard_on_all_targets() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_magic_set_guard(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_magic_set_guard_push:"), "{name}");
            assert!(asm.contains("__rt_magic_set_guard_pop:"), "{name}");
            assert!(asm.contains("_magic_set_guard_head"), "{name}");
            assert!(!asm.contains("_fiber_current"), "{name}");
        }
    }
}
