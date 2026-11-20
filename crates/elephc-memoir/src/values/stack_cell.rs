//! Purpose:
//! Writes the scalar representation of a borrowed Mixed cell in a native frame.
//!
//! Called from:
//! - Compiler arithmetic and scalar boxing with frame-proven lifetimes.
//!
//! Key details:
//! - The compiler selects the tag, value, and frame placement.

use crate::{abi, FrameOffset, TargetAbi};

/// Where a frame cell's runtime tag comes from.
pub enum CellTag<'a> {
    /// A tag selected at compile time.
    Immediate(i64),
    /// A tag already in a native register.
    Register(&'a str),
}

/// Lays out one 24-byte frame cell: payload at `offset - 8`, tag at `offset`, a cleared
/// word at `offset - 16`. The payload register is clobbered.
pub fn emit_scalar_cell(emitter: &mut impl TargetAbi, cell: FrameOffset, payload: &str, tag: CellTag<'_>) {
    let offset = cell.bytes();
    abi::store_at_offset(emitter, payload, offset - 8);
    match tag {
        CellTag::Immediate(tag) => {
            abi::emit_load_int_immediate(emitter, payload, tag);
            abi::store_at_offset(emitter, payload, offset);
        }
        CellTag::Register(tag) => abi::store_at_offset(emitter, tag, offset),
    }
    abi::emit_load_int_immediate(emitter, payload, 0);
    abi::store_at_offset(emitter, payload, offset - 16);
}
