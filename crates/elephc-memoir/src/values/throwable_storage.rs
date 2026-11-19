//! Purpose:
//! Emits the compact native Throwable payload layout and cold-path initialization.
//!
//! Called from:
//! - Runtime error constructors and compiled Throwable lowering.
//!
//! Key details:
//! - The caller supplies the class and message, and publishes the completed payload.
//! - Recycled allocations require explicit initialization of each populated slot.

use crate::{Arch, Emitter, TargetAbi};

/// Native storage size for a complete compact Throwable payload.
pub const PAYLOAD_BYTES: u64 = 56;
/// Width of the class id word preceding the message pair.
const CLASS_ID: usize = 0;
/// Offset of the message pointer.
const MESSAGE_POINTER: usize = 8;
/// Offset of the message length.
const MESSAGE_LENGTH: usize = 16;
/// Offset of PHP's numeric exception code.
const CODE: usize = 24;
/// Offset of the creation line stored alongside the code.
pub const CREATION_LINE_OFFSET: u64 = 32;
/// Offset of the owned previous Throwable pointer.
pub const PREVIOUS: usize = 40;

/// Encoding of the x86-64 static message length store used by a cold path.
#[derive(Clone, Copy)]
pub enum LengthStore {
    /// Store the length immediate directly into the payload.
    Immediate,
    /// Stage the length in the scratch register used to initialize the message.
    Register,
}

/// Stores the class id supplied by the program's class table.
pub fn emit_class_id(emitter: &mut impl TargetAbi, symbol: &str) {
    let payload = emitter.int_result_reg();
    let scratch = match emitter.arch() { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    emitter.emit_load_symbol_to_reg(scratch, symbol, 0);
    emitter.emit_store_to_address(scratch, payload, CLASS_ID);
}

/// Stores an already prepared message pair at the physical Throwable offsets.
pub fn emit_message_pair(emitter: &mut impl TargetAbi, pointer: &str, length: &str) {
    let payload = emitter.int_result_reg();
    emitter.emit_store_to_address(pointer, payload, MESSAGE_POINTER);
    emitter.emit_store_to_address(length, payload, MESSAGE_LENGTH);
}

/// Borrows a static diagnostic and installs its complete message pair.
pub fn emit_static_message(emitter: &mut impl Emitter, symbol: &str, length: i64, store: LengthStore) {
    let payload = emitter.int_result_reg();
    let scratch = match emitter.arch() { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    emitter.emit_symbol_address(scratch, symbol);
    emitter.emit_store_to_address(scratch, payload, MESSAGE_POINTER);
    if emitter.arch() == Arch::X86_64 && matches!(store, LengthStore::Immediate) {
        emitter.instruction(&format!("mov QWORD PTR [rax + {MESSAGE_LENGTH}], {length}")); // store the complete static diagnostic length
    } else {
        emitter.emit_load_int_immediate(scratch, length);
        emitter.emit_store_to_address(scratch, payload, MESSAGE_LENGTH);
    }
}

/// Initializes the exception code of a fresh compact error to zero.
pub fn emit_default_code(emitter: &mut impl TargetAbi, payload: &str) {
    emitter.emit_store_zero_to_address(payload, CODE);
}

/// Writes the marker used when a Throwable has no recorded creation line.
pub fn emit_throwable_creation_line_unknown(emitter: &mut impl TargetAbi, payload: &str) {
    emitter.emit_store_zero_to_address(payload, CREATION_LINE_OFFSET as usize);
}

/// Initializes the previous-owner slot to null.
pub fn emit_empty_previous(emitter: &mut impl TargetAbi, payload: &str) {
    emitter.emit_store_zero_to_address(payload, PREVIOUS);
}

/// Initializes the default code, location marker and empty previous-owner slot.
pub fn emit_default_fields(emitter: &mut impl TargetAbi, payload: &str) {
    emit_default_code(emitter, payload);
    emit_throwable_creation_line_unknown(emitter, payload);
    emit_empty_previous(emitter, payload);
}
