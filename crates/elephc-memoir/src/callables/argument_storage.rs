//! Purpose:
//! Builds and retires framed packed argument vectors.
//!
//! Called from:
//! - Native callback adapters with a packed argument vector.
//!
//! Key details:
//! - Boxing holds its own reference, so both owners must be consumed.

use crate::{abi, values::array_header, Arch, Emitter, ValueTag};

/// Allocates an indexed argument array of boxed Mixed cells.
///
/// The requested capacity must already be in the first native integer argument register.
pub fn emit_allocate_mixed_array(emitter: &mut impl Emitter) {
    let element_size = abi::int_arg_reg_name(emitter, 1);
    abi::emit_load_int_immediate(emitter, element_size, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    let result = abi::int_result_reg(emitter);
    array_header::emit_array_value_type_stamp(emitter, result, ValueTag::new(7));
}

/// Boxes one owned argument container and retires its independent raw owner.
pub fn emit_box_owned_container<E, R>(
    emitter: &mut E,
    container: &str,
    destination: &str,
    tag: ValueTag,
    release_container: R,
)
where
    E: Emitter,
    R: FnOnce(&mut E),
{
    abi::emit_push_reg(emitter, container);
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_reg_move(emitter, "x1", container);
            abi::emit_load_int_immediate(emitter, "x0", tag.raw() as i64);
            abi::emit_load_int_immediate(emitter, "x2", 0);
        }
        Arch::X86_64 => {
            abi::emit_reg_move(emitter, "rdi", container);
            abi::emit_load_int_immediate(emitter, "rax", tag.raw() as i64);
            abi::emit_load_int_immediate(emitter, "rsi", 0);
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    emitter.emit_load_temporary_stack_slot(result, 16);
    release_container(emitter);
    abi::emit_pop_reg(emitter, destination);
    emitter.emit_release_temporary_stack(16);
}

/// Releases the box before the independent array owner at the caller's target-specific offsets.
pub fn emit_release_boxed_array(emitter: &mut impl Emitter, boxed: usize, array: usize) {
    for (slot, release) in [(boxed, "__rt_decref_mixed"), (array, "__rt_decref_any")] {
        emit_load_owner(emitter, slot);
        abi::emit_call_label(emitter, release);
    }
}

/// Appends a cell supplied in the native argument registers, then consumes its staged owner.
pub fn emit_append_owned_cell(emitter: &mut impl Emitter, array: usize, cell: usize) {
    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
    emit_save_owner(emitter, array);
    emit_load_owner(emitter, cell);
    abi::emit_call_label(emitter, "__rt_decref_any");
}

/// Appends an owned boxed cell from registers and returns the possibly grown array.
pub fn emit_append_owned_cell_from_registers(
    emitter: &mut impl Emitter,
    array: &str,
    cell: &str,
    destination: &str,
) {
    abi::emit_push_reg(emitter, cell);
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_reg_move(emitter, "x0", array);
            abi::emit_reg_move(emitter, "x1", cell);
        }
        Arch::X86_64 => {
            abi::emit_reg_move(emitter, "rdi", array);
            abi::emit_reg_move(emitter, "rsi", cell);
        }
    }
    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    emitter.emit_load_temporary_stack_slot(result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(emitter, destination);
    emitter.emit_release_temporary_stack(16);
}

/// Boxes a completed argument array and retires its separate raw owner.
pub fn emit_box_arguments(emitter: &mut impl Emitter, array: usize, boxed: usize) {
    let result = abi::int_result_reg(emitter);
    emit_load_owner(emitter, array);
    emit_box_owned_container(
        emitter,
        result,
        result,
        ValueTag::new(4),
        |emitter| abi::emit_call_label(emitter, "__rt_decref_any"),
    );
    emit_save_owner(emitter, boxed);
}

/// Loads an owned value in the native result register.
fn emit_load_owner(emitter: &mut impl Emitter, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("ldr x0, [sp, #{slot}]")), // reload an ARM frame owner
        Arch::X86_64 => emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {slot}]")), // reload an x86 frame owner
    }
}

/// Stores the current owner before another native operation can observe the frame.
fn emit_save_owner(emitter: &mut impl Emitter, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("str x0, [sp, #{slot}]")), // publish the ARM frame owner
        Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rbp - {slot}], rax")), // publish the x86 frame owner
    }
}
