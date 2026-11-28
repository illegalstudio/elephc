//! Purpose:
//! Links and removes the borrowed storage of dynamic property write guards.
//!
//! Called from:
//! - The native magic property write dispatch adapter.
//!
//! Key details:
//! - A guard borrows the receiver and name for the lifetime of its caller's frame.

use crate::owner_layout::MagicSetGuard;
use crate::{Arch, Emitter, abi};

/// Current coroutine's head for the borrowed guard chain.
pub const HEAD: &str = "_magic_set_guard_head";

/// Emits publication and removal of one caller-owned magic-setter guard node.
pub fn emit_link_and_pop(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Publishes a borrowed magic-setter node and emits aarch64 unlink traversal.
fn emit_aarch64(emitter: &mut impl Emitter) {
    emitter.label("__rt_magic_set_guard_push_new");
    emitter.instruction("str x9, [x3]");                                        // node.next = old head
    emitter.instruction(&format!("str x0, [x3, #{}]", MagicSetGuard::RECEIVER)); // node.receiver = receiver
    emitter.instruction(&format!("str x1, [x3, #{}]", MagicSetGuard::NAME_POINTER)); // node.name_ptr = property bytes
    emitter.instruction(&format!("str x2, [x3, #{}]", MagicSetGuard::NAME_LENGTH)); // node.name_len = property length
    abi::emit_store_reg_to_symbol(
        emitter,
        "x3",
        HEAD,
        0,
    );
    emitter.instruction("mov x0, #1");                                          // this caller owns the new guard node
    emitter.instruction("ret");                                                 // emit the guarded operation

    emitter.label_global("__rt_magic_set_guard_pop");
    abi::emit_symbol_address(emitter, "x9", HEAD);
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

/// Publishes a borrowed magic-setter node and emits x86_64 unlink traversal.
fn emit_x86_64(emitter: &mut impl Emitter) {
    emitter.label("__rt_magic_set_guard_push_new");
    abi::emit_load_symbol_to_reg(
        emitter,
        "r8",
        HEAD,
        0,
    );
    emitter.instruction("mov QWORD PTR [rcx], r8");                             // node.next = old head
    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], rdi", MagicSetGuard::RECEIVER)); // node.receiver = receiver
    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], rsi", MagicSetGuard::NAME_POINTER)); // node.name_ptr = property bytes
    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], rdx", MagicSetGuard::NAME_LENGTH)); // node.name_len = property length
    abi::emit_store_reg_to_symbol(
        emitter,
        "rcx",
        HEAD,
        0,
    );
    emitter.instruction("mov eax, 1");                                          // this caller owns the new guard node
    emitter.instruction("ret");                                                 // emit the guarded operation

    emitter.label_global("__rt_magic_set_guard_pop");
    abi::emit_symbol_address(emitter, "r8", HEAD);
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
