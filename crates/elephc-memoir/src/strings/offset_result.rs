//! Purpose:
//! Stages and publishes owned string bytes produced by an offset write.
//!
//! Called from:
//! - The compiler's boxed string-offset setter.
//!
//! Key details:
//! - The original result is retired only when persistence returns a different base.
//! - The caller proves that the target cell still owns its original payload.

use crate::{Arch, Emitter, layout};

/// Slots used by ARM64 and SysV for the same string replacement transaction.
pub struct OffsetResult {
    /// Slot holding the target Mixed cell.
    pub cell: usize,
    /// Slot holding the new string pointer, followed by its length.
    pub pointer: usize,
}

impl OffsetResult {
    /// Persists the current string result and leaves the owner pair in the caller's frame.
    pub fn emit_persist(&self, emitter: &mut impl Emitter, owned: &str) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("stp x1, x2, [sp, #{}]", self.pointer)); // keep the temporary result across persistence
                emitter.emit_call_label("__rt_str_persist");
                emitter.emit_load_from_address("x3", "sp", self.pointer);
                emitter.instruction(&format!("stp x1, x2, [sp, #{}]", self.pointer)); // park the independent owner and byte count
                emitter.instruction("cmp x1, x3");                              // check whether the temporary was adopted in place
                emitter.instruction(&format!("b.eq {owned}"));                  // an adopted block stays live under the new owner
                emitter.instruction("mov x0, x3");                              // retire the old temporary only after copying it
            }
            Arch::X86_64 => {
                emitter.store_at_offset("rax", self.pointer);
                emitter.emit_call_label("__rt_str_persist");
                emitter.load_at_offset("r10", self.pointer);
                emitter.store_at_offset("rax", self.pointer);
                emitter.store_at_offset("rdx", self.pointer + 8);
                emitter.instruction("cmp rax, r10");                            // check whether persistence adopted the temporary
                emitter.instruction(&format!("je {owned}"));                    // leave an adopted block owned by the replacement
                emitter.instruction("mov rax, r10");                            // retire only the temporary that was copied
            }
        }
        emitter.emit_call_label("__rt_heap_free_safe");
        emitter.label(owned);
    }

    /// Drops a pending offset write if its retained cell is the last remaining owner.
    pub fn emit_cancel_if_detached(&self, emitter: &mut impl Emitter, retained: usize, cast: usize) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.emit_load_from_address("x9", "sp", retained);                                   // reload the retained-cell flag
                emitter.instruction("cbz x9, __rt_mixed_array_set_string_publish"); // no warning could have run a handler
                emitter.emit_load_from_address("x10", "sp", self.cell);                                   // reload the target Mixed cell
                crate::heap_refcount::emit_load_owner_count(emitter, "w11", "x10");
                emitter.instruction("cmp w11, #1");                             // is the retained reference the last one?
                emitter.instruction("b.ne __rt_mixed_array_set_string_publish"); // the variable still holds the cell: write it
                emitter.emit_load_from_address("x0", "sp", self.pointer);                                   // a handler replaced the variable: drop the updated string
                emitter.instruction("bl __rt_heap_free_safe");                  // php abandons the write in that case
                emitter.emit_load_from_address("x0", "sp", cast);                                   // the cast value string
                emitter.instruction("bl __rt_heap_free_safe");                  // release it (no-op for scratch)
                emitter.emit_load_from_address("x0", "sp", self.cell);                                    // the abandoned cell
                emitter.instruction("bl __rt_decref_mixed");                    // release the last reference, freeing it
                emitter.instruction("b __rt_mixed_array_set_done");             // the value was already consumed
                emitter.label("__rt_mixed_array_set_string_publish");
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp QWORD PTR [rbp - {retained}], 0")); // was the cell retained?
                emitter.instruction("je __rt_mixed_array_set_string_publish");  // no warning could have run a handler
                emitter.load_at_offset("r10", self.cell);                        // reload the target Mixed cell
                crate::heap_refcount::emit_compare_owner_count_x86_64(emitter, "r10", 1);
                emitter.instruction("jne __rt_mixed_array_set_string_publish"); // the variable still holds the cell: write it
                emitter.load_at_offset("rax", self.pointer);                       // a handler replaced the variable: drop the updated string
                emitter.instruction("call __rt_heap_free_safe");                // php abandons the write in that case
                emitter.load_at_offset("rax", cast);                       // the cast value string
                emitter.instruction("call __rt_heap_free_safe");                // release it (no-op for scratch)
                emitter.load_at_offset("rax", self.cell);                        // the abandoned cell
                emitter.instruction("call __rt_decref_mixed");                  // release the last reference, freeing it
                emitter.instruction("jmp __rt_mixed_array_set_done");           // the value was already consumed
                emitter.label("__rt_mixed_array_set_string_publish");
            }
        }
    }

    /// Replaces the cell's exclusive string payload with the staged owner pair.
    pub fn emit_publish(&self, emitter: &mut impl Emitter) {
        let result = emitter.int_result_reg();
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x10, [sp, #{}]", self.cell)); // reload the selected cell before retiring its payload
                layout::emit_load_mixed_low(emitter, result, "x10");
                emitter.emit_call_label("__rt_heap_free_safe");
                emitter.instruction(&format!("ldr x10, [sp, #{}]", self.cell)); // recover the cell after the release call
                emitter.instruction(&format!("ldp x1, x2, [sp, #{}]", self.pointer)); // recover the staged replacement pointer and length
                emitter.emit_store_to_address("x1", "x10", 8);
                emitter.emit_store_to_address("x2", "x10", 16);
            }
            Arch::X86_64 => {
                emitter.load_at_offset("r10", self.cell);
                layout::emit_load_mixed_low(emitter, result, "r10");
                emitter.emit_call_label("__rt_heap_free_safe");
                emitter.load_at_offset("r10", self.cell);
                emitter.load_at_offset(result, self.pointer);
                emitter.emit_store_to_address(result, "r10", 8);
                emitter.load_at_offset(result, self.pointer + 8);
                emitter.emit_store_to_address(result, "r10", 16);
            }
        }
    }
}
