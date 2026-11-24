//! Purpose:
//! Emits the compact native Throwable payload layout and cold-path initialization.
//!
//! Called from:
//! - Runtime error constructors and compiled Throwable lowering.
//!
//! Key details:
//! - The caller supplies the class and message, and publishes the completed payload.
//! - Recycled allocations require explicit initialization of each populated slot.

use crate::{abi, Arch, Emitter, FrameOffset, TargetAbi};

/// Native storage size for a complete compact Throwable payload.
const PAYLOAD_BYTES: i64 = 56;
/// Offset of the class id word preceding the message pair.
const CLASS_ID: usize = 0;
/// Offset of the message pointer.
pub const MESSAGE_POINTER: usize = 8;
/// Offset of the message length.
pub const MESSAGE_LENGTH: usize = 16;
/// Offset of PHP's numeric exception code.
pub const CODE: usize = 24;
/// Offset of the creation line stored alongside the code.
pub const CREATION_LINE_OFFSET: u64 = 32;
/// Offset of the owned previous Throwable pointer.
pub const PREVIOUS: usize = 40;
/// Offset of the auxiliary word used by cold error paths.
const AUXILIARY: usize = 48;

/// Allocates a compact Throwable with one owner and an assigned native handle.
pub fn emit_allocate(emitter: &mut impl Emitter) {
    emitter.emit_load_int_immediate(emitter.int_result_reg(), PAYLOAD_BYTES);
    super::object_storage::emit_allocate_throwable(emitter);
}

/// Allocates a compact Throwable using the original x86-64 cold-path encoding.
pub fn emit_allocate_cold(emitter: &mut impl Emitter, x86_size_register: &str) {
    match emitter.arch() {
        Arch::AArch64 => emit_allocate(emitter),
        Arch::X86_64 => {
            emitter.emit_load_int_immediate(x86_size_register, PAYLOAD_BYTES);
            super::object_storage::emit_allocate_throwable_x86_64(emitter);
        }
    }
}

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
    emit_store_message_pair(emitter, emitter.int_result_reg(), pointer, length);
}

/// Stores a prepared message pair into a compact Throwable at an arbitrary base register.
pub fn emit_store_message_pair(emitter: &mut impl TargetAbi, payload: &str, pointer: &str, length: &str) {
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

/// Detaches the default message and previous owners before constructor initialization.
pub fn emit_release_constructor_defaults(emitter: &mut impl Emitter, object_frame_offset: FrameOffset) {
    let object = emitter.symbol_scratch_reg();
    let value = emitter.int_result_reg();
    for (offset, release) in [(MESSAGE_POINTER, "__rt_heap_free_safe"), (PREVIOUS, "__rt_decref_any")] {
        emitter.load_at_offset(object, object_frame_offset.bytes());
        emitter.emit_load_from_address(value, object, offset);
        // Detach before release, since a previous exception may run a user destructor.
        emitter.emit_store_zero_to_address(object, offset);
        if offset == MESSAGE_POINTER {
            emitter.emit_store_zero_to_address(object, MESSAGE_LENGTH);
        }
        emitter.emit_call_label(release);
    }
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

/// Builds a compact error with a zeroed auxiliary word using the caller ABI scratch register.
pub fn emit_error_with_auxiliary_zero(
    emitter: &mut impl Emitter, class_symbol: &str, message: &str, length: i64,
) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, result, PAYLOAD_BYTES);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    let kind = match emitter.arch() {
        Arch::AArch64 => crate::layout::THROWABLE_HEAP_KIND as i64,
        Arch::X86_64 => crate::layout::x86_64_heap_kind_word(crate::layout::THROWABLE_HEAP_KIND) as i64,
    };
    abi::emit_load_int_immediate(emitter, scratch, kind);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("stur x10, [x0, #-8]"),            // stamp the allocated payload as a Throwable object
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rax - 8], r10"),    // stamp the allocated payload as a Throwable object
    }
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
    abi::emit_load_symbol_to_reg(emitter, scratch, class_symbol, 0);
    abi::emit_store_to_address(emitter, scratch, result, CLASS_ID);
    abi::emit_symbol_address(emitter, scratch, message);
    abi::emit_store_to_address(emitter, scratch, result, MESSAGE_POINTER);
    abi::emit_load_int_immediate(emitter, scratch, length);
    abi::emit_store_to_address(emitter, scratch, result, MESSAGE_LENGTH);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    for offset in [CODE, PREVIOUS, AUXILIARY] {
        abi::emit_store_to_address(emitter, scratch, result, offset);
    }
    emit_throwable_creation_line_unknown(emitter, result);
}

/// Projects borrowed compact Throwable fields into a four-word C result buffer.
pub fn emit_c_projection(emitter: &mut impl Emitter) {
    emitter.label_global(&emitter.extern_symbol("elephc_phpext_native_throwable_fields"));
    let object = emitter.int_arg_reg_name(0);
    let output = emitter.int_arg_reg_name(1);
    let scratch = emitter.secondary_scratch_reg();
    for (index, offset) in [CLASS_ID, MESSAGE_POINTER, MESSAGE_LENGTH, CODE].into_iter().enumerate() {
        emitter.emit_load_from_address(scratch, object, offset);
        emitter.emit_store_to_address(scratch, output, index * 8);
    }
    emitter.emit_return();
}
