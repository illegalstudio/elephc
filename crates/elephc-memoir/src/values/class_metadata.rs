//! Purpose:
//! Defines generated native class lookup rows shared by compiler writers and runtime readers.
//!
//! Called from:
//! - Compiler data emission for dynamic class construction and `instanceof` targets.
//! - Native object, eval, and dynamic `instanceof` lookup helpers.
//!
//! Key details:
//! - Dynamic construction and `instanceof` rows use four native words with distinct typed fields.
//! - Dense class-name rows use two native words indexed by runtime class id.
//! - PHP name matching and target-kind semantics remain outside Memoir.

use std::mem::{offset_of, size_of};

use crate::{Arch, Emitter};

/// One generated dynamic class-construction lookup row.
#[repr(C)]
pub struct ClassLookupRow<T> {
    /// Class name pointer.
    pub name: T,
    /// Class name byte count.
    pub name_len: T,
    /// Runtime class id.
    pub class_id: T,
    /// Object payload byte count.
    pub object_size: T,
}

/// One generated dynamic `instanceof` target row.
#[repr(C)]
pub struct InstanceofTargetRow<T> {
    /// Target class-like name pointer.
    pub name: T,
    /// Target class-like name byte count.
    pub name_len: T,
    /// Runtime class or interface id.
    pub target_id: T,
    /// Target kind discriminator.
    pub target_kind: T,
}

/// One dense runtime class-id to class-name row.
#[repr(C)]
pub struct ClassNameRow<T> {
    /// Class name pointer.
    pub name: T,
    /// Class name byte count.
    pub name_len: T,
}

impl<T> ClassLookupRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [self.name, self.name_len, self.class_id, self.object_size]
    }
}

impl<T> InstanceofTargetRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [self.name, self.name_len, self.target_id, self.target_kind]
    }
}

impl<T> ClassNameRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 2] {
        [self.name, self.name_len]
    }
}

/// Selects one word from a dynamic class-construction lookup row.
#[derive(Clone, Copy)]
pub enum ClassLookupField {
    /// Class name pointer.
    Name,
    /// Class name byte count.
    NameLength,
    /// Runtime class id.
    ClassId,
    /// Object payload byte count.
    ObjectSize,
}

/// Selects one word from a dynamic `instanceof` target row.
#[derive(Clone, Copy)]
pub enum InstanceofTargetField {
    /// Target class-like name pointer.
    Name,
    /// Target class-like name byte count.
    NameLength,
    /// Runtime class or interface id.
    TargetId,
    /// Target kind discriminator.
    TargetKind,
}

/// Selects one word from a dense class-name row.
#[derive(Clone, Copy)]
pub enum ClassNameField {
    /// Class name pointer.
    Name,
    /// Class name byte count.
    NameLength,
}

impl ClassLookupField {
    /// Returns the field displacement inside one class lookup row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(ClassLookupRow<u64>, name),
            Self::NameLength => offset_of!(ClassLookupRow<u64>, name_len),
            Self::ClassId => offset_of!(ClassLookupRow<u64>, class_id),
            Self::ObjectSize => offset_of!(ClassLookupRow<u64>, object_size),
        }
    }
}

impl InstanceofTargetField {
    /// Returns the field displacement inside one dynamic target row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(InstanceofTargetRow<u64>, name),
            Self::NameLength => offset_of!(InstanceofTargetRow<u64>, name_len),
            Self::TargetId => offset_of!(InstanceofTargetRow<u64>, target_id),
            Self::TargetKind => offset_of!(InstanceofTargetRow<u64>, target_kind),
        }
    }
}

impl ClassNameField {
    /// Returns the field displacement inside one dense class-name row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(ClassNameRow<u64>, name),
            Self::NameLength => offset_of!(ClassNameRow<u64>, name_len),
        }
    }
}

/// Bytes occupied by one generated dynamic class-construction lookup row.
pub const CLASS_LOOKUP_ROW_BYTES: usize = size_of::<ClassLookupRow<u64>>();
/// Bytes occupied by one generated dynamic `instanceof` target row.
pub const INSTANCEOF_TARGET_ROW_BYTES: usize = size_of::<InstanceofTargetRow<u64>>();
/// Bytes occupied by one dense class-name row.
pub const CLASS_NAME_ROW_BYTES: usize = size_of::<ClassNameRow<u64>>();

const _: () = assert!(CLASS_LOOKUP_ROW_BYTES == 32);
const _: () = assert!(INSTANCEOF_TARGET_ROW_BYTES == 32);
const _: () = assert!(CLASS_NAME_ROW_BYTES == 16);

/// Loads one typed field from a dynamic class-construction lookup row.
pub fn emit_load_class_lookup_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: ClassLookupField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Advances a dynamic class-construction lookup cursor to the next row.
pub fn emit_advance_class_lookup_row(emitter: &mut impl Emitter, row: &str) {
    emit_advance_row(emitter, row, CLASS_LOOKUP_ROW_BYTES);
}

/// Loads one typed field from a dynamic `instanceof` target row.
pub fn emit_load_instanceof_target_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: InstanceofTargetField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Advances a dynamic `instanceof` target cursor to the next row.
pub fn emit_advance_instanceof_target_row(emitter: &mut impl Emitter, row: &str) {
    emit_advance_row(emitter, row, INSTANCEOF_TARGET_ROW_BYTES);
}

/// Forms the address of one dense class-name row by runtime class id.
pub fn emit_class_name_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    class_id: &str,
    scratch: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {destination}, {table}, {class_id}, lsl #4"));
        }
        Arch::X86_64 => {
            let offset = if destination == table { scratch } else { destination };
            emitter.emit_reg_move(offset, class_id);
            emitter.instruction(&format!("shl {offset}, 4"));
            if destination == table {
                emitter.instruction(&format!("add {destination}, {offset}"));
            } else {
                emitter.instruction(&format!("add {destination}, {table}"));
            }
        }
    }
}

/// Loads one typed field from a selected dense class-name row.
pub fn emit_load_class_name_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: ClassNameField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Loads the pointer and byte count from a selected dense class-name row.
pub fn emit_load_class_name_pair(
    emitter: &mut impl Emitter,
    pointer: &str,
    length: &str,
    row: &str,
) {
    emit_load_class_name_field(emitter, pointer, row, ClassNameField::Name);
    emit_load_class_name_field(emitter, length, row, ClassNameField::NameLength);
}

/// Loads the number of dense runtime class-name rows.
pub fn emit_load_class_name_count(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_load_symbol_to_reg(destination, "_class_name_count", 0);
}

/// Loads the number of dense runtime class-name rows through a local symbol reference.
pub fn emit_load_local_class_name_count(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_local_symbol_address(destination, "_class_name_count");
    emitter.emit_load_from_address(destination, destination, 0);
}

/// Compares a runtime class id with the number of dense class-name rows.
pub fn emit_cmp_class_id_to_name_count(emitter: &mut impl Emitter, class_id: &str) {
    emitter.emit_cmp_reg_to_symbol(class_id, "_class_name_count");
}

/// Materializes the address of the dense runtime class-name row table.
pub fn emit_class_name_table_address(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_symbol_address(destination, "_class_name_entries");
}

/// Materializes the local address of the dense runtime class-name row table.
pub fn emit_local_class_name_table_address(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_local_symbol_address(destination, "_class_name_entries");
}

/// Materializes the address of the shared empty class-name sentinel.
pub fn emit_missing_class_name_address(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_symbol_address(destination, "_class_name_missing");
}

/// Materializes the local address of the shared empty class-name sentinel.
pub fn emit_local_missing_class_name_address(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_local_symbol_address(destination, "_class_name_missing");
}

/// Advances a generated metadata cursor by one fixed-size row.
fn emit_advance_row(emitter: &mut impl Emitter, row: &str, row_bytes: usize) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("add {row}, {row}, #{row_bytes}")),
        Arch::X86_64 => emitter.instruction(&format!("add {row}, {row_bytes}")),
    }
}
