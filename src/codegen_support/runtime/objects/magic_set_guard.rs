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

use elephc_memoir::owner_layout::MagicSetGuard;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits guard insertion and removal helpers for dynamic-name magic property writes.
pub(crate) fn emit_magic_set_guard(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits AArch64 receiver/property reentrancy-guard push and pop helpers.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: magic __set reentrancy guard ---");
    emitter.label_global("__rt_magic_set_guard_push");
    crate::codegen_support::abi::emit_load_symbol_to_reg(
        emitter,
        "x9",
        elephc_memoir::reference_cells::magic_set_guard::HEAD,
        0,
    );
    emitter.instruction("mov x11, x9");                                         // scan the active guard chain from its head
    emitter.label("__rt_magic_set_guard_push_scan");
    emitter.instruction("cbz x11, __rt_magic_set_guard_push_new");              // no matching active receiver/name pair exists
    emitter.instruction(&format!("ldr x12, [x11, #{}]", MagicSetGuard::RECEIVER)); // compare receiver identity
    emitter.instruction("cmp x12, x0");                                         // emit the guarded operation
    emitter.instruction("b.ne __rt_magic_set_guard_push_next");                 // emit the guarded operation
    emitter.instruction(&format!("ldr x12, [x11, #{}]", MagicSetGuard::NAME_LENGTH)); // compare property-name length
    emitter.instruction("cmp x12, x2");                                         // emit the guarded operation
    emitter.instruction("b.ne __rt_magic_set_guard_push_next");                 // emit the guarded operation
    emitter.instruction(&format!("ldr x12, [x11, #{}]", MagicSetGuard::NAME_POINTER)); // x12 = active property bytes
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
    elephc_memoir::reference_cells::magic_set_guard::emit_link_and_pop(emitter);
}

/// Emits x86_64 receiver/property reentrancy-guard push and pop helpers.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: magic __set reentrancy guard ---");
    emitter.label_global("__rt_magic_set_guard_push");
    crate::codegen_support::abi::emit_load_symbol_to_reg(
        emitter,
        "r8",
        elephc_memoir::reference_cells::magic_set_guard::HEAD,
        0,
    );
    emitter.instruction("mov r10, r8");                                         // scan the active guard chain from its head
    emitter.label("__rt_magic_set_guard_push_scan");
    emitter.instruction("test r10, r10");                                       // emit the guarded operation
    emitter.instruction("jz __rt_magic_set_guard_push_new");                    // no matching active receiver/name pair exists
    emitter.instruction(&format!("cmp QWORD PTR [r10 + {}], rdi", MagicSetGuard::RECEIVER)); // compare receiver identity
    emitter.instruction("jne __rt_magic_set_guard_push_next");                  // emit the guarded operation
    emitter.instruction(&format!("cmp QWORD PTR [r10 + {}], rdx", MagicSetGuard::NAME_LENGTH)); // compare property-name length
    emitter.instruction("jne __rt_magic_set_guard_push_next");                  // emit the guarded operation
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", MagicSetGuard::NAME_POINTER)); // r11 = active property bytes
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
    elephc_memoir::reference_cells::magic_set_guard::emit_link_and_pop(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    #[test]
    /// Verifies magic-setter guards include receiver identity and property name on every target.
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
