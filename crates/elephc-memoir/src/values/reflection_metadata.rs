//! Purpose:
//! Defines the generated native metadata rows used by eval reflection lookups.
//!
//! Called from:
//! - Compiler data emission for AOT class, method, and property metadata.
//! - Native eval bridge lookup helpers on AArch64 and x86_64.
//!
//! Key details:
//! - Method and property lookups share one seven-word member row.
//! - Class flag lookups use one three-word class row.
//! - Reflection flag meanings remain compiler semantics outside Memoir.

use std::mem::{offset_of, size_of};

use crate::{Arch, Emitter};

/// One generated method or property reflection metadata row.
#[repr(C)]
pub struct MemberRow<T> {
    /// Reflected class-like name pointer.
    pub class_name: T,
    /// Reflected class-like name byte count.
    pub class_name_len: T,
    /// Reflected method or property name pointer.
    pub member_name: T,
    /// Reflected method or property name byte count.
    pub member_name_len: T,
    /// Reflection predicate and source metadata flags.
    pub flags: T,
    /// Declaring class-like name pointer.
    pub declaring_class: T,
    /// Declaring class-like name byte count.
    pub declaring_class_len: T,
}

/// One generated class-like reflection flag metadata row.
#[repr(C)]
pub struct ClassRow<T> {
    /// Reflected class-like name pointer.
    pub class_name: T,
    /// Reflected class-like name byte count.
    pub class_name_len: T,
    /// Reflection predicate and source metadata flags.
    pub flags: T,
}

/// One generated class-to-related-name reflection metadata row.
#[repr(C)]
pub struct NameRow<T> {
    /// Reflected class-like name pointer.
    pub class_name: T,
    /// Reflected class-like name byte count.
    pub class_name_len: T,
    /// Related method, property, interface, trait, or alias name pointer.
    pub name: T,
    /// Related name byte count.
    pub name_len: T,
}

impl<T> MemberRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 7] {
        [
            self.class_name,
            self.class_name_len,
            self.member_name,
            self.member_name_len,
            self.flags,
            self.declaring_class,
            self.declaring_class_len,
        ]
    }
}

impl<T> ClassRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 3] {
        [self.class_name, self.class_name_len, self.flags]
    }
}

impl<T> NameRow<T> {
    /// Returns the native words in their physical row order.
    pub fn into_words(self) -> [T; 4] {
        [self.class_name, self.class_name_len, self.name, self.name_len]
    }
}

/// Selects one word from a generated member reflection metadata row.
#[derive(Clone, Copy)]
pub enum MemberField {
    /// Reflected class-like name pointer.
    ClassName,
    /// Reflected class-like name byte count.
    ClassNameLength,
    /// Reflected method or property name pointer.
    MemberName,
    /// Reflected method or property name byte count.
    MemberNameLength,
    /// Reflection predicate and source metadata flags.
    Flags,
    /// Declaring class-like name pointer.
    DeclaringClass,
    /// Declaring class-like name byte count.
    DeclaringClassLength,
}

/// Selects one word from a generated class reflection metadata row.
#[derive(Clone, Copy)]
pub enum ClassField {
    /// Reflected class-like name pointer.
    ClassName,
    /// Reflected class-like name byte count.
    ClassNameLength,
    /// Reflection predicate and source metadata flags.
    Flags,
}

/// Selects one word from the common prefix used by reflection name scans.
#[derive(Clone, Copy)]
pub enum NameField {
    /// Reflected class-like name pointer.
    ClassName,
    /// Reflected class-like name byte count.
    ClassNameLength,
    /// Related member or class-like name pointer.
    Name,
    /// Related name byte count.
    NameLength,
}

/// Selects the physical row shape traversed by a reflection name scan.
#[derive(Clone, Copy)]
pub enum NameRowKind {
    /// A seven-word method or property metadata row.
    Member,
    /// A four-word class/interface, class/trait, or class/alias relation row.
    Relation,
}

impl MemberField {
    /// Returns the field displacement inside one native member row.
    const fn offset(self) -> usize {
        match self {
            Self::ClassName => offset_of!(MemberRow<u64>, class_name),
            Self::ClassNameLength => offset_of!(MemberRow<u64>, class_name_len),
            Self::MemberName => offset_of!(MemberRow<u64>, member_name),
            Self::MemberNameLength => offset_of!(MemberRow<u64>, member_name_len),
            Self::Flags => offset_of!(MemberRow<u64>, flags),
            Self::DeclaringClass => offset_of!(MemberRow<u64>, declaring_class),
            Self::DeclaringClassLength => offset_of!(MemberRow<u64>, declaring_class_len),
        }
    }
}

impl ClassField {
    /// Returns the field displacement inside one native class row.
    const fn offset(self) -> usize {
        match self {
            Self::ClassName => offset_of!(ClassRow<u64>, class_name),
            Self::ClassNameLength => offset_of!(ClassRow<u64>, class_name_len),
            Self::Flags => offset_of!(ClassRow<u64>, flags),
        }
    }
}

impl NameField {
    /// Returns the field displacement inside the common name-row prefix.
    const fn offset(self) -> usize {
        match self {
            Self::ClassName => offset_of!(NameRow<u64>, class_name),
            Self::ClassNameLength => offset_of!(NameRow<u64>, class_name_len),
            Self::Name => offset_of!(NameRow<u64>, name),
            Self::NameLength => offset_of!(NameRow<u64>, name_len),
        }
    }
}

impl NameRowKind {
    /// Returns the byte stride of the selected physical row shape.
    const fn row_bytes(self) -> usize {
        match self {
            Self::Member => MEMBER_ROW_BYTES,
            Self::Relation => NAME_ROW_BYTES,
        }
    }
}

/// Bytes occupied by one generated member reflection metadata row.
pub const MEMBER_ROW_BYTES: usize = size_of::<MemberRow<u64>>();
/// Bytes occupied by one generated class reflection metadata row.
pub const CLASS_ROW_BYTES: usize = size_of::<ClassRow<u64>>();
/// Bytes occupied by one generated class-to-related-name metadata row.
pub const NAME_ROW_BYTES: usize = size_of::<NameRow<u64>>();

const _: () = assert!(MEMBER_ROW_BYTES == 56);
const _: () = assert!(CLASS_ROW_BYTES == 24);
const _: () = assert!(NAME_ROW_BYTES == 32);
const _: () = assert!(offset_of!(MemberRow<u64>, class_name) == offset_of!(NameRow<u64>, class_name));
const _: () = assert!(offset_of!(MemberRow<u64>, class_name_len) == offset_of!(NameRow<u64>, class_name_len));
const _: () = assert!(offset_of!(MemberRow<u64>, member_name) == offset_of!(NameRow<u64>, name));
const _: () = assert!(offset_of!(MemberRow<u64>, member_name_len) == offset_of!(NameRow<u64>, name_len));

/// Loads one typed field from a selected member reflection metadata row.
pub fn emit_load_member_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: MemberField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Advances a member reflection metadata cursor to the next row.
pub fn emit_advance_member_row(emitter: &mut impl Emitter, row: &str) {
    emit_advance_row(emitter, row, MEMBER_ROW_BYTES);
}

/// Loads one typed field from a selected class reflection metadata row.
pub fn emit_load_class_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: ClassField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Advances a class reflection metadata cursor to the next row.
pub fn emit_advance_class_row(emitter: &mut impl Emitter, row: &str) {
    emit_advance_row(emitter, row, CLASS_ROW_BYTES);
}

/// Loads one typed field from the common reflection name-row prefix.
pub fn emit_load_name_field(
    emitter: &mut impl Emitter,
    destination: &str,
    row: &str,
    field: NameField,
) {
    emitter.emit_load_from_address(destination, row, field.offset());
}

/// Advances a reflection name scan across its selected physical row shape.
pub fn emit_advance_name_row(
    emitter: &mut impl Emitter,
    row: &str,
    kind: NameRowKind,
) {
    emit_advance_row(emitter, row, kind.row_bytes());
}

/// Advances a generated metadata cursor by one fixed-size row.
fn emit_advance_row(emitter: &mut impl Emitter, row: &str, row_bytes: usize) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("add {row}, {row}, #{row_bytes}")),
        Arch::X86_64 => emitter.instruction(&format!("add {row}, {row_bytes}")),
    }
}
