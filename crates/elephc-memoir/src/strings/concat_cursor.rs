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
        emit_read(emitter, scratch);
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
        emit_write(emitter, scratch);
    }
}

/// Materializes the arena base for byte access by a scratch producer.
pub fn emit_base_address(emitter: &mut impl TargetAbi, register: &str) {
    emitter.emit_symbol_address(register, "_concat_buf");
}

/// Materializes the physical cursor slot for a producer retaining its address.
pub fn emit_cursor_address(emitter: &mut impl TargetAbi, register: &str) {
    emitter.emit_symbol_address(register, "_concat_off");
}

/// Loads the current byte cursor for a scoped save or a bounds check.
pub fn emit_read(emitter: &mut impl TargetAbi, register: &str) {
    emitter.emit_load_symbol_to_reg(register, "_concat_off", 0);
}

/// Installs a previously saved cursor or a checked producer's completed position.
pub fn emit_write(emitter: &mut impl TargetAbi, register: &str) {
    emitter.emit_store_reg_to_symbol(register, "_concat_off", 0);
}

/// Resets the arena cursor when the surrounding boundary releases its scratch lifetime.
pub fn emit_reset(emitter: &mut impl TargetAbi) {
    emitter.emit_store_zero_to_symbol("_concat_off", 0);
}

/// Uses the immediate-store form at the inline native export boundary.
pub fn emit_boundary_reset(emitter: &mut impl TargetAbi) {
    emitter.emit_store_imm_to_symbol("_concat_off", 0, 0);
}
