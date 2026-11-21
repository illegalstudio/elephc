//! Purpose:
//! Preserves the initialized concat prefix across a call that can reuse scratch.
//!
//! Called from:
//! - Compiler emitters that temporarily yield the scratch arena to a callback.
//!
//! Key details:
//! - Frame slots retain the length and the independent heap snapshot.

use crate::{Arch, Emitter};

/// Frame operands for a saved scratch prefix.
pub struct PrefixCheckpoint<'a> {
    /// Frame operand holding the prefix length.
    pub length: &'a str,
    /// Frame operand holding the heap snapshot.
    pub owner: &'a str,
}

impl PrefixCheckpoint<'_> {
    /// Reads the current scratch cursor into the checkpoint.
    pub fn emit_capture(&self, emitter: &mut impl Emitter) {
        let Self { length, .. } = self;
        if emitter.arch() == Arch::AArch64 {
            super::concat_cursor::emit_cursor_address(emitter, "x9");
            emitter.instruction("ldr x10, [x9]");                               // capture the caller-visible concat-buffer offset before the user method runs
            emitter.instruction(&format!("str x10, {length}"));                 // save the captured offset (= prefix length) across the user method invocation
        } else {
            super::concat_cursor::emit_read(emitter, "r10");             // capture the caller-visible concat-buffer offset before the user method runs
            emitter.instruction(&format!("mov QWORD PTR {length}, r10"));       // save the captured offset (= prefix length) across the user method invocation
        }
    }

    /// Copies the prefix into independent storage before invoking a callback.
    pub fn emit_save(&self, emitter: &mut impl Emitter, invoke: &str) {
        let Self { length, owner } = self;
        if emitter.arch() == Arch::AArch64 {
            emitter.instruction(&format!("str xzr, {owner}"));                  // default the saved prefix heap pointer to null for empty prefixes
            emitter.instruction(&format!("ldr x10, {length}"));                 // recover the prefix length after saving unrelated JSON state
            emitter.instruction(&format!("cbz x10, {invoke}"));                 // skip the prefix copy when there is no caller-visible prefix to preserve
            super::concat_cursor::emit_base_address(emitter, "x1");
            emitter.instruction("mov x2, x10");                                 // copy the prefix length into the str_persist length register
            emitter.instruction("bl __rt_str_persist");                         // duplicate the caller prefix into a heap-owned buffer
            emitter.instruction(&format!("str x1, {owner}"));                   // remember the heap-owned prefix pointer for the post-call restore
            emitter.label(invoke);
        } else {
            emitter.instruction(&format!("mov QWORD PTR {owner}, 0"));          // default the saved prefix heap pointer to null for empty prefixes
            emitter.instruction(&format!("mov r10, QWORD PTR {length}"));       // recover the prefix length after saving unrelated JSON state
            emitter.instruction("test r10, r10");                               // is there any caller-visible prefix to preserve?
            emitter.instruction(&format!("jz {invoke}"));                       // skip the prefix copy when the prefix is empty
            super::concat_cursor::emit_base_address(emitter, "rax");                    // materialize the concat-buffer base for the str_persist input
            emitter.instruction("mov rdx, r10");                                // copy the prefix length into the str_persist length register
            emitter.instruction("call __rt_str_persist");                       // duplicate the caller prefix into a heap-owned buffer
            emitter.instruction(&format!("mov QWORD PTR {owner}, rax"));        // remember the heap-owned prefix pointer for the post-call restore
            emitter.label(invoke);
        }
    }

    /// Restores the prefix and cursor, then consumes the saved owner.
    pub fn emit_restore(&self, emitter: &mut impl Emitter, copy: &str, done: &str) {
        let Self { length, owner } = self;
        if emitter.arch() == Arch::AArch64 {
            // Copy the heap-owned prefix back into concat_buf[0..prefix_len].
            emitter.instruction(&format!("ldr x10, {length}"));                 // reload the saved prefix length
            emitter.instruction(&format!("cbz x10, {done}"));                   // skip the prefix restore when no prefix was preserved
            emitter.instruction(&format!("ldr x11, {owner}"));                  // reload the heap-owned prefix pointer
            emitter.instruction(&format!("cbz x11, {done}"));                   // defensive null guard for the heap-owned prefix pointer
            super::concat_cursor::emit_base_address(emitter, "x12");
            emitter.instruction("mov x13, #0");                                 // initialize the prefix copy index
            emitter.label(copy);
            emitter.instruction("cmp x13, x10");                                // have we copied every prefix byte back?
            emitter.instruction(&format!("b.ge {done}"));                       // exit once the entire prefix is restored
            emitter.instruction("ldrb w14, [x11, x13]");                        // load the next prefix byte from the heap-owned copy
            emitter.instruction("strb w14, [x12, x13]");                        // restore the byte into concat_buf at the original offset
            emitter.instruction("add x13, x13, #1");                            // advance the prefix copy index
            emitter.instruction(&format!("b {copy}"));                          // continue the prefix restore loop
            emitter.label(done);
            emitter.instruction(&format!("ldr x0, {owner}"));                   // load the owned prefix copy after its bytes have been restored
            emitter.instruction("bl __rt_decref_any");                          // release the temporary string copy, including the null-prefix case
            super::concat_cursor::emit_cursor_address(emitter, "x9");
            emitter.instruction(&format!("ldr x10, {length}"));                 // reload the saved pre-call concat-buffer offset
            emitter.instruction("str x10, [x9]");                               // restore the concat-buffer offset for the next append
        } else {
            // Copy the heap-owned prefix back into concat_buf[0..prefix_len].
            emitter.instruction(&format!("mov r10, QWORD PTR {length}"));       // reload the saved prefix length
            emitter.instruction("test r10, r10");                               // anything to restore?
            emitter.instruction(&format!("jz {done}"));                         // skip restore for empty prefixes
            emitter.instruction(&format!("mov r11, QWORD PTR {owner}"));        // reload the heap-owned prefix pointer
            emitter.instruction("test r11, r11");                               // defensive null guard for the heap-owned prefix pointer
            emitter.instruction(&format!("jz {done}"));                         // skip restore when no heap copy was made
            super::concat_cursor::emit_base_address(emitter, "r9");                     // materialize the concat-buffer base for the prefix restore loop
            emitter.instruction("xor rcx, rcx");                                // initialize the prefix copy index
            emitter.label(copy);
            emitter.instruction("cmp rcx, r10");                                // have we copied every prefix byte back?
            emitter.instruction(&format!("jae {done}"));                        // exit once the entire prefix is restored
            emitter.instruction("mov dl, BYTE PTR [r11 + rcx]");                // load the next prefix byte from the heap-owned copy
            emitter.instruction("mov BYTE PTR [r9 + rcx], dl");                 // restore the byte into concat_buf at the original offset
            emitter.instruction("add rcx, 1");                                  // advance the prefix copy index
            emitter.instruction(&format!("jmp {copy}"));                        // continue the prefix restore loop
            emitter.label(done);
            emitter.instruction(&format!("mov rax, QWORD PTR {owner}"));        // load the owned prefix copy after its bytes have been restored
            emitter.instruction("call __rt_decref_any");                        // release the temporary string copy, including the null-prefix case
            emitter.instruction(&format!("mov r10, QWORD PTR {length}"));       // reload the saved pre-call concat-buffer offset
            super::concat_cursor::emit_write(emitter, "r10");            // restore the concat-buffer offset for the next append
        }
    }
}
