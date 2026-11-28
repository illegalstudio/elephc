//! Purpose:
//! Defines and projects the native property rows shared by object diagnostic renderers.
//!
//! Called from:
//! - Compiler data emission for per-class property tables.
//! - Native `var_dump`, `print_r`, and `var_export` object walkers.
//! - Native serialization and hydration of declared object properties.
//!
//! Key details:
//! - Each table starts with one row-count word followed by uniform six-word records.
//! - The final pair is renderer-specific text, either a type name or a bare property name.
//! - Serialization uses a distinct four-word record with the same leading count word.
//! - Hydration diagnostics use a parallel four-word record without a leading count word.
//! - JSON projection uses a three-word class header followed by four-word property records.

use std::mem::{offset_of, size_of};

use crate::{Arch, Emitter};

/// One native property descriptor row.
#[repr(C)]
pub struct PropertyDescriptor<T> {
    /// Renderer-specific key pointer.
    pub key: T,
    /// Renderer-specific key byte count.
    pub key_len: T,
    /// Byte displacement of the physical property slot inside the object.
    pub object_offset: T,
    /// Logical value tag plus native storage flags.
    pub value_tag: T,
    /// Renderer-specific auxiliary text pointer.
    pub auxiliary: T,
    /// Renderer-specific auxiliary text byte count.
    pub auxiliary_len: T,
}

/// One native serialized-property descriptor row.
#[repr(C)]
pub struct SerializedPropertyDescriptor<T> {
    /// PHP-mangled property key pointer.
    pub key: T,
    /// PHP-mangled property key byte count.
    pub key_len: T,
    /// Byte displacement of the physical property slot inside the object.
    pub object_offset: T,
    /// Logical value tag plus native storage flags.
    pub value_tag: T,
}

/// One native serialized-property hydration diagnostic row.
#[repr(C)]
pub struct HydrationDiagnostic<T> {
    /// Declared-type message suffix pointer.
    pub suffix: T,
    /// Declared-type message suffix byte count.
    pub suffix_len: T,
    /// Bit mask of accepted boxed runtime value tags.
    pub accepted_tags: T,
    /// Whether an accepted integer is widened into a float slot.
    pub widen_int_to_float: T,
}

/// One native JSON class descriptor header.
#[repr(C)]
pub struct JsonClassDescriptor<T> {
    /// Runtime projection flags.
    pub flags: T,
    /// Optional `jsonSerialize` or native projection target.
    pub projection_target: T,
    /// Number of public property rows following this header.
    pub property_count: T,
}

/// One native JSON property descriptor row.
#[repr(C)]
pub struct JsonPropertyDescriptor<T> {
    /// Public property name pointer.
    pub name: T,
    /// Public property name byte count.
    pub name_len: T,
    /// Physical property slot index inside the object.
    pub object_index: T,
    /// Logical value tag plus native storage flags.
    pub value_tag: T,
}

impl<T> JsonClassDescriptor<T> {
    /// Returns the native words in their physical header order.
    pub fn into_words(self) -> [T; 3] {
        [self.flags, self.projection_target, self.property_count]
    }
}

impl<T> JsonPropertyDescriptor<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [self.name, self.name_len, self.object_index, self.value_tag]
    }
}

impl<T> HydrationDiagnostic<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [
            self.suffix,
            self.suffix_len,
            self.accepted_tags,
            self.widen_int_to_float,
        ]
    }
}

impl<T> SerializedPropertyDescriptor<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [self.key, self.key_len, self.object_offset, self.value_tag]
    }
}

impl<T> PropertyDescriptor<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 6] {
        [
            self.key,
            self.key_len,
            self.object_offset,
            self.value_tag,
            self.auxiliary,
            self.auxiliary_len,
        ]
    }
}

/// Selects one word from a projected property descriptor row.
#[derive(Clone, Copy)]
pub enum Field {
    /// Renderer-specific key pointer.
    Key,
    /// Renderer-specific key byte count.
    KeyLength,
    /// Byte displacement of the property slot inside the object.
    ObjectOffset,
    /// Logical value tag plus native storage flags.
    ValueTag,
    /// Renderer-specific auxiliary text pointer.
    Auxiliary,
    /// Renderer-specific auxiliary text byte count.
    AuxiliaryLength,
}

/// Selects one word from a projected serialized-property descriptor row.
#[derive(Clone, Copy)]
pub enum SerializedField {
    /// PHP-mangled property key pointer.
    Key,
    /// PHP-mangled property key byte count.
    KeyLength,
    /// Byte displacement of the property slot inside the object.
    ObjectOffset,
    /// Logical value tag plus native storage flags.
    ValueTag,
}

/// Selects one word from a serialized-property hydration diagnostic row.
#[derive(Clone, Copy)]
pub enum HydrationField {
    /// Declared-type message suffix pointer.
    Suffix,
    /// Declared-type message suffix byte count.
    SuffixLength,
    /// Bit mask of accepted boxed runtime value tags.
    AcceptedTags,
    /// Whether an accepted integer is widened into a float slot.
    WidenIntToFloat,
}

/// Selects one word from a native JSON class descriptor header.
#[derive(Clone, Copy)]
pub enum JsonHeaderField {
    /// Runtime projection flags.
    Flags,
    /// Optional `jsonSerialize` or native projection target.
    ProjectionTarget,
    /// Number of public property rows.
    PropertyCount,
}

/// Selects one word from a native JSON property descriptor row.
#[derive(Clone, Copy)]
pub enum JsonPropertyField {
    /// Public property name pointer.
    Name,
    /// Public property name byte count.
    NameLength,
    /// Physical property slot index inside the object.
    ObjectIndex,
    /// Logical value tag plus native storage flags.
    ValueTag,
}

impl JsonHeaderField {
    /// Returns the field displacement inside one native header.
    const fn offset(self) -> usize {
        match self {
            Self::Flags => offset_of!(JsonClassDescriptor<u64>, flags),
            Self::ProjectionTarget => offset_of!(JsonClassDescriptor<u64>, projection_target),
            Self::PropertyCount => offset_of!(JsonClassDescriptor<u64>, property_count),
        }
    }
}

impl JsonPropertyField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Name => offset_of!(JsonPropertyDescriptor<u64>, name),
            Self::NameLength => offset_of!(JsonPropertyDescriptor<u64>, name_len),
            Self::ObjectIndex => offset_of!(JsonPropertyDescriptor<u64>, object_index),
            Self::ValueTag => offset_of!(JsonPropertyDescriptor<u64>, value_tag),
        }
    }
}

impl HydrationField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Suffix => offset_of!(HydrationDiagnostic<u64>, suffix),
            Self::SuffixLength => offset_of!(HydrationDiagnostic<u64>, suffix_len),
            Self::AcceptedTags => offset_of!(HydrationDiagnostic<u64>, accepted_tags),
            Self::WidenIntToFloat => {
                offset_of!(HydrationDiagnostic<u64>, widen_int_to_float)
            }
        }
    }
}

impl SerializedField {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Key => offset_of!(SerializedPropertyDescriptor<u64>, key),
            Self::KeyLength => offset_of!(SerializedPropertyDescriptor<u64>, key_len),
            Self::ObjectOffset => offset_of!(SerializedPropertyDescriptor<u64>, object_offset),
            Self::ValueTag => offset_of!(SerializedPropertyDescriptor<u64>, value_tag),
        }
    }
}

impl Field {
    /// Returns the field displacement inside one native row.
    const fn offset(self) -> usize {
        match self {
            Self::Key => offset_of!(PropertyDescriptor<u64>, key),
            Self::KeyLength => offset_of!(PropertyDescriptor<u64>, key_len),
            Self::ObjectOffset => offset_of!(PropertyDescriptor<u64>, object_offset),
            Self::ValueTag => offset_of!(PropertyDescriptor<u64>, value_tag),
            Self::Auxiliary => offset_of!(PropertyDescriptor<u64>, auxiliary),
            Self::AuxiliaryLength => offset_of!(PropertyDescriptor<u64>, auxiliary_len),
        }
    }
}

/// Bytes occupied by the leading property-count word.
pub const COUNT_BYTES: usize = size_of::<u64>();
/// Bytes occupied by one property descriptor row.
pub const ROW_BYTES: usize = size_of::<PropertyDescriptor<u64>>();
/// Bytes occupied by one serialized-property descriptor row.
pub const SERIALIZED_ROW_BYTES: usize = size_of::<SerializedPropertyDescriptor<u64>>();
/// Bytes occupied by one serialized-property hydration diagnostic row.
pub const HYDRATION_ROW_BYTES: usize = size_of::<HydrationDiagnostic<u64>>();
/// Bytes occupied by one JSON class descriptor header.
pub const JSON_HEADER_BYTES: usize = size_of::<JsonClassDescriptor<u64>>();
/// Bytes occupied by one JSON property descriptor row.
pub const JSON_PROPERTY_ROW_BYTES: usize = size_of::<JsonPropertyDescriptor<u64>>();

const _: () = assert!(COUNT_BYTES == 8);
const _: () = assert!(ROW_BYTES == 48);
const _: () = assert!(SERIALIZED_ROW_BYTES == 32);
const _: () = assert!(HYDRATION_ROW_BYTES == 32);
const _: () = assert!(JSON_HEADER_BYTES == 24);
const _: () = assert!(JSON_PROPERTY_ROW_BYTES == 32);

/// Loads the row count from a validated property descriptor table.
pub fn emit_load_count(emitter: &mut impl Emitter, destination: &str, table: &str) {
    emitter.emit_load_from_address(destination, table, 0);
}

/// Forms the address of a selected row after the table's leading count word.
pub fn emit_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_load_int_immediate(scratch, ROW_BYTES as i64);
            emitter.instruction(&format!("mul {scratch}, {index}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {table}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {destination}, #{COUNT_BYTES}"));
        }
        Arch::X86_64 => {
            let offset = if destination == table { scratch } else { destination };
            emitter.emit_reg_move(offset, index);
            emitter.instruction(&format!("imul {offset}, {offset}, {ROW_BYTES}"));
            if destination == table {
                emitter.instruction(&format!("add {destination}, {offset}"));
            } else {
                emitter.instruction(&format!("add {destination}, {table}"));
            }
            emitter.instruction(&format!("add {destination}, {COUNT_BYTES}"));
        }
    }
}

/// Loads one typed field from an already selected property descriptor row.
pub fn emit_load_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: Field,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Forms the address of a serialized-property row after the table's leading count word.
pub fn emit_serialized_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_load_int_immediate(scratch, SERIALIZED_ROW_BYTES as i64);
            emitter.instruction(&format!("mul {scratch}, {index}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {table}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {destination}, #{COUNT_BYTES}"));
        }
        Arch::X86_64 => {
            let offset = if destination == table { scratch } else { destination };
            emitter.emit_reg_move(offset, index);
            emitter.instruction(&format!(
                "imul {offset}, {offset}, {SERIALIZED_ROW_BYTES}"
            ));
            if destination == table {
                emitter.instruction(&format!("add {destination}, {offset}"));
            } else {
                emitter.instruction(&format!("add {destination}, {table}"));
            }
            emitter.instruction(&format!("add {destination}, {COUNT_BYTES}"));
        }
    }
}

/// Loads one typed field from an already selected serialized-property row.
pub fn emit_load_serialized_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: SerializedField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Forms the address of a selected serialized-property hydration diagnostic row.
pub fn emit_hydration_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    table: &str,
    index: &str,
    scratch: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_load_int_immediate(scratch, HYDRATION_ROW_BYTES as i64);
            emitter.instruction(&format!("mul {scratch}, {index}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {table}, {scratch}"));
        }
        Arch::X86_64 => {
            let offset = if destination == table { scratch } else { destination };
            emitter.emit_reg_move(offset, index);
            emitter.instruction(&format!("imul {offset}, {offset}, {HYDRATION_ROW_BYTES}"));
            if destination == table {
                emitter.instruction(&format!("add {destination}, {offset}"));
            } else {
                emitter.instruction(&format!("add {destination}, {table}"));
            }
        }
    }
}

/// Loads one typed field from an already selected hydration diagnostic row.
pub fn emit_load_hydration_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: HydrationField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Loads one typed field from a native JSON class descriptor header.
pub fn emit_load_json_header_field(
    emitter: &mut impl Emitter,
    destination: &str,
    descriptor: &str,
    field: JsonHeaderField,
) {
    emitter.emit_load_from_address(destination, descriptor, field.offset());
}

/// Forms the address of a JSON property row after its class descriptor header.
pub fn emit_json_property_row_address(
    emitter: &mut impl Emitter,
    destination: &str,
    descriptor: &str,
    index: &str,
    scratch: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_load_int_immediate(scratch, JSON_PROPERTY_ROW_BYTES as i64);
            emitter.instruction(&format!("mul {scratch}, {index}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {descriptor}, {scratch}"));
            emitter.instruction(&format!("add {destination}, {destination}, #{JSON_HEADER_BYTES}"));
        }
        Arch::X86_64 => {
            let offset = if destination == descriptor { scratch } else { destination };
            emitter.emit_reg_move(offset, index);
            emitter.instruction(&format!(
                "imul {offset}, {offset}, {JSON_PROPERTY_ROW_BYTES}"
            ));
            if destination == descriptor {
                emitter.instruction(&format!("add {destination}, {offset}"));
            } else {
                emitter.instruction(&format!("add {destination}, {descriptor}"));
            }
            emitter.instruction(&format!("add {destination}, {JSON_HEADER_BYTES}"));
        }
    }
}

/// Loads one typed field from an already selected JSON property descriptor row.
pub fn emit_load_json_property_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: JsonPropertyField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}
