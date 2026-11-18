//! Purpose:
//! Saves and restores borrowed registry pairs in a frame-owned stack snapshot.
//!
//! Called from:
//! - A serialization boundary isolating a nested native call.
//!
//! Key details:
//! - Only the populated prefixes are copied from the two source tables.
//! - The interleaved snapshot uses sixteen bytes per pair and acquires no new owners.
//! - The enclosing frame restores its stack pointer after restoring the live tables.

use crate::{Arch, Emitter, FrameOffset, abi};

/// Identifies the two registry columns and the outer frame's snapshot metadata.
pub struct StackRegistrySnapshot<'a> {
    /// Symbol of the borrowed pointer column.
    pub pointers: &'a str,
    /// Symbol of the corresponding index column.
    pub indices: &'a str,
    /// Frame slot containing the bounded populated count.
    pub count: FrameOffset,
    /// Frame slot that holds the snapshot base across callbacks.
    pub saved: FrameOffset,
    /// Prefix of the direction-specific copy loop labels.
    pub label_prefix: &'a str,
}

impl StackRegistrySnapshot<'_> {
    /// Copies the populated pairs between the live columns and the owning frame.
    pub fn emit_copy(&self, emitter: &mut impl Emitter, restore: bool) {
        let phase = if restore { "restore" } else { "save" };
        let loop_label = format!("{}_{phase}_loop", self.label_prefix);
        let done = format!("{}_{phase}_done", self.label_prefix);
        // -- copy the populated registry prefix to or from its aligned stack snapshot --
        match emitter.arch() {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x12", self.count.bytes());
                if restore {
                    abi::load_at_offset(emitter, "x16", self.saved.bytes());
                } else {
                    emitter.instruction("lsl x13, x12, #4");                    // reserve two words for every registered object
                    emitter.instruction("sub sp, sp, x13");                     // keep the snapshot below the stable fixed frame
                    emitter.instruction("mov x16, sp");                         // remember the aligned snapshot payload
                    abi::store_at_offset(emitter, "x16", self.saved.bytes());
                }
                abi::emit_symbol_address(emitter, "x9", self.pointers);
                abi::emit_symbol_address(emitter, "x10", self.indices);
                emitter.instruction("mov x11, #0");                             // start at the populated registry prefix
                emitter.label(&loop_label);
                emitter.instruction("cmp x11, x12");                            // test the bounded object count
                emitter.instruction(&format!("b.hs {done}"));                   // finish after every used pair is copied
                emitter.instruction("add x15, x16, x11, lsl #4");               // address the interleaved snapshot pair
                if restore {
                    emitter.instruction("ldp x13, x14, [x15]");                 // recover the outer object identity and reference index
                    emitter.instruction("str x13, [x9, x11, lsl #3]");          // restore the outer object pointer
                    emitter.instruction("str x14, [x10, x11, lsl #3]");         // restore its PHP reference index
                } else {
                    emitter.instruction("ldr x13, [x9, x11, lsl #3]");          // read the outer object pointer
                    emitter.instruction("ldr x14, [x10, x11, lsl #3]");         // read its PHP reference index
                    emitter.instruction("stp x13, x14, [x15]");                 // snapshot the pair without acquiring a new owner
                }
                emitter.instruction("add x11, x11, #1");                        // advance to the next registered object
                emitter.instruction(&format!("b {loop_label}"));                // continue copying the bounded prefix
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r8", self.count.bytes());
                if restore {
                    abi::load_at_offset(emitter, "r9", self.saved.bytes());
                } else {
                    emitter.instruction("mov r9, r8");                          // begin sizing two words per registered object
                    emitter.instruction("shl r9, 4");                           // retain sixteen-byte stack alignment
                    emitter.instruction("sub rsp, r9");                         // reserve the bounded snapshot on the native stack
                    emitter.instruction("mov r9, rsp");                         // remember its payload independently of nested frames
                    abi::store_at_offset(emitter, "r9", self.saved.bytes());
                }
                abi::emit_symbol_address(emitter, "r10", self.pointers);
                abi::emit_symbol_address(emitter, "r11", self.indices);
                emitter.instruction("xor ecx, ecx");                            // start with the first populated registry pair
                emitter.label(&loop_label);
                emitter.instruction("cmp rcx, r8");                             // test the bounded object count
                emitter.instruction(&format!("jae {done}"));                    // finish after the populated prefix
                emitter.instruction("mov rax, rcx");                            // compute the interleaved snapshot stride
                emitter.instruction("shl rax, 4");                              // each saved pointer/index pair occupies sixteen bytes
                emitter.instruction("add rax, r9");                             // address this saved pair
                if restore {
                    emitter.instruction("mov rdx, QWORD PTR [rax]");            // recover the outer object identity
                    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");        // recover its PHP reference index
                    emitter.instruction("mov QWORD PTR [r10 + rcx*8], rdx");    // restore the outer object pointer
                    emitter.instruction("mov QWORD PTR [r11 + rcx*8], rsi");    // restore its reference index
                } else {
                    emitter.instruction("mov rdx, QWORD PTR [r10 + rcx*8]");    // read the outer object identity
                    emitter.instruction("mov rsi, QWORD PTR [r11 + rcx*8]");    // read its PHP reference index
                    emitter.instruction("mov QWORD PTR [rax], rdx");            // snapshot the borrowed object identity
                    emitter.instruction("mov QWORD PTR [rax + 8], rsi");        // snapshot its reference index
                }
                emitter.instruction("inc rcx");                                 // advance to the next registered object
                emitter.instruction(&format!("jmp {loop_label}"));              // continue copying the bounded prefix
            }
        }
        emitter.label(&done);
    }
}
