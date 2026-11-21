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

/// Aligned storage reserved per borrowed call-argument cell.
pub const BORROWED_MIXED_ARG_CELL_BYTES: usize = (crate::layout::MIXED_CELL_SIZE + 15) & !15;

/// Writes a borrowed scalar cell once the address and original payload are materialized.
pub fn emit_borrowed_argument(emitter: &mut impl TargetAbi, cell: &str, payload: &str, tag: i64) {
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, tag);
    emitter.emit_store_to_address(result, cell, 0);
    emitter.emit_store_to_address(payload, cell, 8);
    emitter.emit_store_zero_to_address(cell, 16);
}

/// Stages a writable headerless container cell at an SP or negative FP displacement.
pub fn emit_container_receiver(
    emitter: &mut impl crate::Emitter, offset: usize, tag: &str, payload: &str,
) {
    match emitter.arch() {
        crate::Arch::AArch64 => {
            emitter.emit_store_to_sp(tag, offset);
            emitter.emit_store_to_sp(payload, offset + 8);
            emitter.emit_store_zero_to_address("sp", offset + 16);
        }
        crate::Arch::X86_64 => {
            emitter.store_at_offset(tag, offset);
            emitter.store_at_offset(payload, offset - 8);
            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", offset - 16)); // clear the unused high word of the receiver
        }
    }
}

/// Reloads a staged receiver's possibly relocated container address.
pub fn emit_load_container_receiver(emitter: &mut impl crate::Emitter, offset: usize, result: &str) {
    match emitter.arch() {
        crate::Arch::AArch64 => emitter.emit_load_from_address(result, "sp", offset + 8),
        crate::Arch::X86_64 => emitter.load_at_offset(result, offset - 8),
    }
}
