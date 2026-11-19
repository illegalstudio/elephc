//! Purpose:
//! Saves and restores the shared concatenation scratch cursor.
//!
//! Called from:
//! - Native PHP frames, nested calls and C export boundaries.
//!
//! Key details:
//! - The caller selects storage and persists any escaping scratch-backed result first.

use crate::{FrameOffset, TargetAbi};

/// Selects the native word holding a saved concat cursor.
#[derive(Clone, Copy)]
pub enum SavedConcatCursor {
    /// Fixed slot in the active native frame.
    Frame(FrameOffset),
    /// Slot relative to the current temporary stack pointer.
    Stack(usize),
    /// Register-sized value pushed onto the temporary stack.
    Pushed,
}

impl SavedConcatCursor {
    /// Captures the active cursor in the selected storage.
    pub fn emit_save(self, emitter: &mut impl TargetAbi, scratch: &str) {
        emitter.emit_load_symbol_to_reg(scratch, "_concat_off", 0);
        match self {
            Self::Frame(offset) => emitter.store_at_offset(scratch, offset.bytes()),
            Self::Stack(offset) => emitter.emit_store_to_sp(scratch, offset),
            Self::Pushed => emitter.emit_push_reg(scratch),
        }
    }

    /// Restores the saved cursor, releasing a pushed slot when applicable.
    pub fn emit_restore(self, emitter: &mut impl TargetAbi, scratch: &str) {
        match self {
            Self::Frame(offset) => emitter.load_at_offset(scratch, offset.bytes()),
            Self::Stack(offset) => emitter.emit_load_temporary_stack_slot(scratch, offset),
            Self::Pushed => emitter.emit_pop_reg(scratch),
        }
        emitter.emit_store_reg_to_symbol(scratch, "_concat_off", 0);
    }
}
