//! Purpose:
//! Owns the heap-backed snapshots of a nested reference registry.
//!
//! Called from:
//! - Native parser entry and completion after a reentrant call.
//!
//! Key details:
//! - A fixed header holds the preceding snapshot and outer context metadata.
//! - The trailing words copy a caller-bounded prefix of borrowed slots.
//! - A negative saved count keeps the live shared registry count on restore.
//! - A restored snapshot remains owned until the caller retires its nesting state.

use crate::{Arch, Emitter, abi};

const MODE: usize = 8;
const OWNER: usize = 16;
const OWNER_REPR: usize = 24;
const COUNT: usize = 32;
const DEPTH: usize = 40;
const COPIED: usize = 48;
const HEADER_SIZE: usize = 56;

/// The host globals backed by a linked registry snapshot.
pub struct Storage<'a> {
    /// Prefix of local copy and restoration labels, including the empty-chain continuation.
    pub label_prefix: &'a str,
    /// Head of the heap-backed snapshot chain.
    pub head: &'a str,
    /// Word holding the outer context mode.
    pub mode: &'a str,
    /// Word holding the outer context-owned pointer.
    pub owner: &'a str,
    /// Word distinguishing the representation of that owner.
    pub owner_repr: &'a str,
    /// Logical count of registry entries.
    pub count: &'a str,
    /// Current traversal depth.
    pub depth: &'a str,
    /// Base of the live pointer registry.
    pub slots: &'a str,
}

impl Storage<'_> {
    /// Allocates and links a snapshot. The bounded copy count is in x11/r11.
    /// The logical count is staged at [sp] on AArch64 or [rbp - 8] on x86_64.
    pub fn emit_push(&self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => self.emit_push_aarch64(emitter),
            Arch::X86_64 => self.emit_push_x86_64(emitter),
        }
    }

    /// Detaches and restores the current snapshot, or branches to the caller's top-level path.
    /// Saves the detached pointer at [sp, #8] / [rbp - 16] for release after nesting state updates.
    pub fn emit_restore_or_jump(&self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => self.emit_restore_aarch64(emitter),
            Arch::X86_64 => self.emit_restore_x86_64(emitter),
        }
    }

    /// Allocates the header and copies the populated registry before linking it.
    fn emit_push_aarch64(&self, emitter: &mut impl Emitter) {
        emitter.instruction("str x11, [sp, #8]");                               // preserve the copy count across allocation
        emitter.instruction("lsl x0, x11, #3");                                 // convert the copied slot count to bytes
        emitter.instruction(&format!("add x0, x0, #{HEADER_SIZE}"));            // include the seven-word context header
        emitter.instruction("bl __rt_heap_alloc");                              // allocate the linked reentrant context snapshot
        abi::emit_symbol_address(emitter, "x9", self.head);
        emitter.instruction("ldr x12, [x9]");                                   // load the previous context link
        emitter.instruction("str x12, [x0]");                                   // context.prev = previous context
        abi::emit_symbol_address(emitter, "x12", self.mode);
        emitter.instruction("ldr x13, [x12]");                                  // load the outer allowed-class mode
        emitter.instruction(&format!("str x13, [x0, #{MODE}]"));                // snapshot the outer allowed-class mode
        abi::emit_symbol_address(emitter, "x12", self.owner);
        emitter.instruction("ldr x13, [x12]");                                  // load the outer context-owned allow-list
        emitter.instruction(&format!("str x13, [x0, #{OWNER}]"));               // move the outer allow-list reference into the snapshot
        abi::emit_symbol_address(emitter, "x12", self.owner_repr);
        emitter.instruction("ldr x13, [x12]");                                  // load the outer list representation flag
        emitter.instruction(&format!("str x13, [x0, #{OWNER_REPR}]"));          // snapshot the outer list representation flag
        emitter.instruction("ldr x13, [sp]");                                   // recover the outer logical registry count
        emitter.instruction(&format!("str x13, [x0, #{COUNT}]"));               // snapshot the logical registry count
        abi::emit_symbol_address(emitter, "x12", self.depth);
        emitter.instruction("ldr x13, [x12]");                                  // load the outer recursive parser depth
        emitter.instruction(&format!("str x13, [x0, #{DEPTH}]"));               // snapshot the outer parser depth
        emitter.instruction("ldr x11, [sp, #8]");                               // recover the bounded registry copy count
        emitter.instruction(&format!("str x11, [x0, #{COPIED}]"));              // record how many registry slots follow the header
        abi::emit_symbol_address(emitter, "x12", self.slots);
        emitter.instruction("mov x13, #0");                                     // start copying the populated registry prefix
        emitter.label(&format!("{}_begin_copy", self.label_prefix));
        emitter.instruction("cmp x13, x11");                                    // copied every in-bounds outer registry slot?
        emitter.instruction(&format!("b.hs {}_begin_copy_done", self.label_prefix)); // finish once the used prefix is preserved
        emitter.instruction("ldr x14, [x12, x13, lsl #3]");                     // load one outer reference-registry entry
        emitter.instruction(&format!("add x15, x0, #{HEADER_SIZE}"));           // derive the snapshot registry payload base
        emitter.instruction("str x14, [x15, x13, lsl #3]");                     // save the outer reference-registry entry
        emitter.instruction("add x13, x13, #1");                                // advance to the next populated slot
        emitter.instruction(&format!("b {}_begin_copy", self.label_prefix));    // continue copying the used registry prefix
        emitter.label(&format!("{}_begin_copy_done", self.label_prefix));
        emitter.instruction("str x0, [x9]");                                    // publish this snapshot as the current context link
    }

    /// Allocates the header and copies the populated registry before linking it.
    fn emit_push_x86_64(&self, emitter: &mut impl Emitter) {
        emitter.instruction("mov QWORD PTR [rbp - 16], r11");                   // preserve the copy count across allocation
        emitter.instruction("mov rax, r11");                                    // start computing the snapshot allocation size
        emitter.instruction("shl rax, 3");                                      // convert copied slots to bytes
        emitter.instruction(&format!("add rax, {HEADER_SIZE}"));                // include the seven-word context header
        emitter.instruction("call __rt_heap_alloc");                            // allocate the linked reentrant context snapshot
        emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", self.head)); // load the previous context link
        emitter.instruction("mov QWORD PTR [rax], r10");                        // context.prev = previous context
        emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", self.mode)); // load the outer allowed-class mode
        emitter.instruction(&format!("mov QWORD PTR [rax + {MODE}], r10"));     // snapshot the outer allowed-class mode
        emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", self.owner)); // load the outer context-owned allow-list
        emitter.instruction(&format!("mov QWORD PTR [rax + {OWNER}], r10"));    // move the outer allow-list reference into the snapshot
        emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", self.owner_repr)); // load the outer list representation flag
        emitter.instruction(&format!("mov QWORD PTR [rax + {OWNER_REPR}], r10")); // snapshot the outer list representation flag
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // recover the outer logical registry count
        emitter.instruction(&format!("mov QWORD PTR [rax + {COUNT}], r10"));    // snapshot the logical registry count
        emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", self.depth)); // load the outer recursive parser depth
        emitter.instruction(&format!("mov QWORD PTR [rax + {DEPTH}], r10"));    // snapshot the outer parser depth
        emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                   // recover the bounded registry copy count
        emitter.instruction(&format!("mov QWORD PTR [rax + {COPIED}], r11"));   // record how many registry slots follow the header
        emitter.instruction(&format!("lea rdx, [rip + {}]", self.slots));       // load the outer reference-registry base
        emitter.instruction("xor r10d, r10d");                                  // start copying the populated registry prefix
        emitter.label(&format!("{}_begin_copy_x", self.label_prefix));
        emitter.instruction("cmp r10, r11");                                    // copied every in-bounds outer registry slot?
        emitter.instruction(&format!("jae {}_begin_copy_done_x", self.label_prefix)); // finish once the used prefix is preserved
        emitter.instruction("mov rcx, QWORD PTR [rdx + r10 * 8]");              // load one outer reference-registry entry
        emitter.instruction(&format!("mov QWORD PTR [rax + r10 * 8 + {HEADER_SIZE}], rcx")); // save the outer reference-registry entry
        emitter.instruction("add r10, 1");                                      // advance to the next populated slot
        emitter.instruction(&format!("jmp {}_begin_copy_x", self.label_prefix)); // continue copying the used registry prefix
        emitter.label(&format!("{}_begin_copy_done_x", self.label_prefix));
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], rax", self.head)); // publish this snapshot as the current context link
    }

    /// Restores the outer metadata and prefix after unlinking the snapshot.
    fn emit_restore_aarch64(&self, emitter: &mut impl Emitter) {
        abi::emit_symbol_address(emitter, "x9", self.head);
        emitter.instruction("ldr x10, [x9]");                                   // load the outer snapshot, if this call was reentrant
        emitter.instruction(&format!("cbz x10, {}_end_top", self.label_prefix)); // top-level completion has no outer state to restore
        emitter.instruction("str x10, [sp, #8]");                               // preserve the snapshot pointer across heap release
        emitter.instruction("ldr x11, [x10]");                                  // load the previous linked context
        emitter.instruction("str x11, [x9]");                                   // pop the current context snapshot
        abi::emit_symbol_address(emitter, "x11", self.mode);
        emitter.instruction(&format!("ldr x12, [x10, #{MODE}]"));               // recover the outer allowed-class mode
        emitter.instruction("str x12, [x11]");                                  // restore the outer allowed-class mode
        abi::emit_symbol_address(emitter, "x11", self.owner);
        emitter.instruction(&format!("ldr x12, [x10, #{OWNER}]"));              // recover the outer owned allow-list reference
        emitter.instruction("str x12, [x11]");                                  // republish the outer owned allow-list
        abi::emit_symbol_address(emitter, "x11", self.owner_repr);
        emitter.instruction(&format!("ldr x12, [x10, #{OWNER_REPR}]"));         // recover the outer list representation flag
        emitter.instruction("str x12, [x11]");                                  // restore the outer list representation flag
        abi::emit_symbol_address(emitter, "x11", self.count);
        emitter.instruction(&format!("ldr x12, [x10, #{COUNT}]"));              // recover the outer logical registry count
        emitter.instruction(&format!("tbnz x12, #63, {}_end_keep_count", self.label_prefix)); // retain indices added by a nested Serializable callback
        emitter.instruction("str x12, [x11]");                                  // restore the outer logical registry count
        emitter.label(&format!("{}_end_keep_count", self.label_prefix));
        abi::emit_symbol_address(emitter, "x11", self.depth);
        emitter.instruction(&format!("ldr x12, [x10, #{DEPTH}]"));              // recover the suspended outer parser depth
        emitter.instruction("str x12, [x11]");                                  // restore the suspended outer parser depth
        emitter.instruction(&format!("ldr x12, [x10, #{COPIED}]"));             // load the bounded registry snapshot length
        abi::emit_symbol_address(emitter, "x11", self.slots);
        emitter.instruction("mov x13, #0");                                     // start restoring the outer registry prefix
        emitter.label(&format!("{}_end_copy", self.label_prefix));
        emitter.instruction("cmp x13, x12");                                    // restored every snapshotted registry slot?
        emitter.instruction(&format!("b.hs {}_end_copy_done", self.label_prefix)); // finish when the full used prefix is live again
        emitter.instruction(&format!("add x14, x10, #{HEADER_SIZE}"));          // derive the snapshot registry payload base
        emitter.instruction("ldr x15, [x14, x13, lsl #3]");                     // load one saved outer registry entry
        emitter.instruction("str x15, [x11, x13, lsl #3]");                     // restore the outer reference-registry entry
        emitter.instruction("add x13, x13, #1");                                // advance to the next saved slot
        emitter.instruction(&format!("b {}_end_copy", self.label_prefix));      // continue restoring the used registry prefix
        emitter.label(&format!("{}_end_copy_done", self.label_prefix));
    }

    /// Restores the outer metadata and prefix after unlinking the snapshot.
    fn emit_restore_x86_64(&self, emitter: &mut impl Emitter) {
        emitter.instruction(&format!("mov r10, QWORD PTR [rip + {}]", self.head)); // load the outer snapshot, if this call was reentrant
        emitter.instruction("test r10, r10");                                   // does an outer parser need restoration?
        emitter.instruction(&format!("jz {}_end_top_x", self.label_prefix));    // top-level completion has no outer state
        emitter.instruction("mov QWORD PTR [rbp - 16], r10");                   // preserve the snapshot pointer across heap release
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the previous linked context
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], r11", self.head)); // pop the current context snapshot
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {MODE}]"));     // recover the outer allowed-class mode
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], r11", self.mode)); // restore the outer allowed-class mode
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {OWNER}]"));    // recover the outer owned allow-list reference
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], r11", self.owner)); // republish the outer owned allow-list
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {OWNER_REPR}]")); // recover the outer list representation flag
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], r11", self.owner_repr)); // restore the outer list representation flag
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {COUNT}]"));    // recover the outer logical registry count
        emitter.instruction("test r11, r11");                                   // negative counts mark a shared legacy registry
        emitter.instruction(&format!("js {}_end_keep_count_x", self.label_prefix)); // retain reference indices consumed by the nested payload
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], r11", self.count)); // restore the outer logical registry count
        emitter.label(&format!("{}_end_keep_count_x", self.label_prefix));
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {DEPTH}]"));    // recover the suspended outer parser depth
        emitter.instruction(&format!("mov QWORD PTR [rip + {}], r11", self.depth)); // restore the suspended outer parser depth
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {COPIED}]"));   // load the bounded registry snapshot length
        emitter.instruction(&format!("lea rdx, [rip + {}]", self.slots));       // load the active reference-registry base
        emitter.instruction("xor ecx, ecx");                                    // start restoring the outer registry prefix
        emitter.label(&format!("{}_end_copy_x", self.label_prefix));
        emitter.instruction("cmp rcx, r11");                                    // restored every snapshotted registry slot?
        emitter.instruction(&format!("jae {}_end_copy_done_x", self.label_prefix)); // finish when the full used prefix is live again
        emitter.instruction(&format!("mov r8, QWORD PTR [r10 + rcx * 8 + {HEADER_SIZE}]")); // load one saved outer registry entry
        emitter.instruction("mov QWORD PTR [rdx + rcx * 8], r8");               // restore the outer reference-registry entry
        emitter.instruction("add rcx, 1");                                      // advance to the next saved slot
        emitter.instruction(&format!("jmp {}_end_copy_x", self.label_prefix));  // continue restoring the used registry prefix
        emitter.label(&format!("{}_end_copy_done_x", self.label_prefix));
    }
}

/// Retires the detached snapshot after its outer state has been restored.
pub fn emit_release(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #8]");                            // pass the consumed snapshot to heap_free
            emitter.instruction("bl __rt_heap_free");                           // release the temporary reentrancy snapshot
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // pass the consumed snapshot to heap_free
            emitter.instruction("call __rt_heap_free");                         // release the temporary reentrancy snapshot
        }
    }
}
