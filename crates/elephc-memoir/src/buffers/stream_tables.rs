//! Purpose:
//! Owns fixed native stream tables and their indexed storage operations.
//!
//! Called from:
//! - Stream builtins, runtime stream adapters and web request reset emission.
//!
//! Key details:
//! - Callers select descriptors and retain control over PHP callbacks.
//! - Table declarations and reset byte counts share the same geometry.

use std::mem::{offset_of, size_of};

use crate::{Arch, Emitter, data::StaticStorage};

/// Number of ordinary fd entries in the stream side tables.
pub const FD_CAPACITY: usize = 256;
/// Number of wrapper registrations.
pub const WRAPPER_REGISTRATION_CAPACITY: usize = 64;
/// Number of live synthetic wrapper handles.
pub const WRAPPER_HANDLE_CAPACITY: usize = 256;
/// Number of filter registrations.
pub const FILTER_REGISTRATION_CAPACITY: usize = 128;
/// Shift from a wrapper or filter registration index to a two-pair row.
pub const REGISTRATION_SHIFT: usize = 5;
/// Width of one pair of independently owned registration strings.
pub const REGISTRATION_BYTES: usize = 1 << REGISTRATION_SHIFT;

const WORD: usize = 8;
const HOST_PAIR_SHIFT: usize = 4;
const ZIP_URI_SHIFT: usize = 8;
const ZIP_MODE_SHIFT: usize = 4;

/// Bytes reserved for one synthetic stream URI.
pub const ZIP_URI_SLOT_BYTES: usize = 1 << ZIP_URI_SHIFT;
/// Maximum stored URI bytes, leaving one byte of spare capacity.
pub const ZIP_URI_MAX_BYTES: usize = ZIP_URI_SLOT_BYTES - 1;
/// Bytes reserved for one synthetic stream fopen mode.
pub const ZIP_MODE_SLOT_BYTES: usize = 1 << ZIP_MODE_SHIFT;
/// Maximum stored mode bytes, leaving one byte of spare capacity.
pub const ZIP_MODE_MAX_BYTES: usize = ZIP_MODE_SLOT_BYTES - 1;

/// One per-class user-filter dispatch and storage descriptor.
#[repr(C)]
pub struct UserFilterVtable<T> {
    /// Native `filter` entry.
    pub filter: T,
    /// Optional native `onCreate` entry.
    pub on_create: T,
    /// Optional native `onClose` entry.
    pub on_close: T,
    /// Whether `filter` uses PHP's four-argument brigade contract.
    pub brigade_arity: T,
    /// Byte offset of the declared `params` property, or zero.
    pub params_offset: T,
    /// Bit mask of boxed visible parameter representations.
    pub boxed_params: T,
}

impl<T> UserFilterVtable<T> {
    /// Returns the native words in their physical vtable order.
    pub fn into_words(self) -> [T; 6] {
        [
            self.filter,
            self.on_create,
            self.on_close,
            self.brigade_arity,
            self.params_offset,
            self.boxed_params,
        ]
    }
}

/// Selects one word from a per-class user-filter vtable.
#[derive(Clone, Copy)]
pub enum UserFilterVtableField {
    /// Native `filter` entry.
    Filter,
    /// Optional native `onCreate` entry.
    OnCreate,
    /// Optional native `onClose` entry.
    OnClose,
    /// Whether `filter` uses PHP's four-argument brigade contract.
    BrigadeArity,
    /// Byte offset of the declared `params` property, or zero.
    ParamsOffset,
    /// Bit mask of boxed visible parameter representations.
    BoxedParams,
}

impl UserFilterVtableField {
    /// Returns the field displacement inside one native user-filter vtable.
    const fn offset(self) -> usize {
        match self {
            Self::Filter => offset_of!(UserFilterVtable<u64>, filter),
            Self::OnCreate => offset_of!(UserFilterVtable<u64>, on_create),
            Self::OnClose => offset_of!(UserFilterVtable<u64>, on_close),
            Self::BrigadeArity => offset_of!(UserFilterVtable<u64>, brigade_arity),
            Self::ParamsOffset => offset_of!(UserFilterVtable<u64>, params_offset),
            Self::BoxedParams => offset_of!(UserFilterVtable<u64>, boxed_params),
        }
    }
}

/// Words occupied by one per-class user-filter vtable.
pub const USER_FILTER_VTABLE_WORDS: usize = size_of::<UserFilterVtable<u64>>() / WORD;

const _: () = assert!(USER_FILTER_VTABLE_WORDS == 6);

/// One word inside a wrapper or user-filter registration row.
#[derive(Clone, Copy)]
pub enum RegistrationField {
    /// Pointer to the registered protocol or filter name.
    NamePointer,
    /// Byte length of the registered protocol or filter name.
    NameLength,
    /// Pointer to the registered wrapper or filter class name.
    ClassPointer,
    /// Byte length of the registered wrapper or filter class name.
    ClassLength,
}

impl RegistrationField {
    /// Returns the native byte offset of this field inside a registration row.
    const fn offset(self) -> usize {
        match self {
            Self::NamePointer => 0,
            Self::NameLength => WORD,
            Self::ClassPointer => 2 * WORD,
            Self::ClassLength => 3 * WORD,
        }
    }
}

/// Selects one per-handle synthetic stream string field.
#[derive(Clone, Copy)]
pub enum ZipMetadataString {
    /// Reported stream URI.
    Uri,
    /// Exact fopen mode.
    Mode,
}

impl ZipMetadataString {
    /// Returns the byte storage for this field.
    const fn storage(self) -> StaticStorage {
        match self {
            Self::Uri => ZIP_STREAM_URIS,
            Self::Mode => ZIP_STREAM_MODES,
        }
    }

    /// Returns the parallel word table containing stored byte lengths.
    const fn length_storage(self) -> StaticStorage {
        match self {
            Self::Uri => ZIP_STREAM_URI_LENGTHS,
            Self::Mode => ZIP_STREAM_MODE_LENGTHS,
        }
    }

    /// Returns the shift from a checked handle index to its byte window.
    const fn shift(self) -> usize {
        match self {
            Self::Uri => ZIP_URI_SHIFT,
            Self::Mode => ZIP_MODE_SHIFT,
        }
    }

    /// Returns the maximum number of bytes stored in one window.
    pub const fn max_bytes(self) -> usize {
        match self {
            Self::Uri => ZIP_URI_MAX_BYTES,
            Self::Mode => ZIP_MODE_MAX_BYTES,
        }
    }
}

/// Defines the bytes reserved for one table from its number of rows and stride.
const fn table(symbol: &'static str, count: usize, stride: usize) -> StaticStorage {
    StaticStorage { symbol, bytes: count * stride }
}

pub const USER_WRAPPERS: StaticStorage = table("_user_wrappers", WRAPPER_REGISTRATION_CAPACITY, REGISTRATION_BYTES);
pub const USER_WRAPPER_HANDLES: StaticStorage = table("_user_wrapper_handles", WRAPPER_HANDLE_CAPACITY, WORD);
pub const USER_FILTER_REGISTRY: StaticStorage = table("_user_filter_registry", FILTER_REGISTRATION_CAPACITY, REGISTRATION_BYTES);
pub const USER_FILTER_INSTANCES: StaticStorage = table("_user_filter_instances", FD_CAPACITY, 2 * WORD);
pub const DIR_HANDLES: StaticStorage = table("_dir_handles", FD_CAPACITY, WORD);
pub const GLOB_HANDLES: StaticStorage = table("_glob_handles", FD_CAPACITY, WORD);
pub const POPEN_FILES: StaticStorage = table("_popen_files", FD_CAPACITY, WORD);
pub const ICONV_HANDLES: StaticStorage = table("_iconv_handles", FD_CAPACITY, WORD);
pub const BZIP2_HANDLES: StaticStorage = table("_bzstream_handles", FD_CAPACITY, WORD);
pub const ZLIB_HANDLES: StaticStorage = table("_zstream_handles", FD_CAPACITY, WORD);
pub const READ_FILTERS: StaticStorage = table("_stream_read_filters", FD_CAPACITY, 1);
pub const WRITE_FILTERS: StaticStorage = table("_stream_write_filters", FD_CAPACITY, 1);
pub const EOF_FLAGS: StaticStorage = table("_eof_flags", FD_CAPACITY, 1);
pub const CHUNK_SIZES: StaticStorage = table("_stream_chunk_size", FD_CAPACITY, WORD);
pub const CONNECT_HOSTS: StaticStorage = table("_stream_connect_host", FD_CAPACITY, 1 << HOST_PAIR_SHIFT);
pub const ZIP_STREAM_KINDS: StaticStorage = table("_zip_stream_kind", WRAPPER_HANDLE_CAPACITY, 1);
pub const ZIP_STREAM_URI_LENGTHS: StaticStorage = table("_zip_stream_uri_len", WRAPPER_HANDLE_CAPACITY, WORD);
pub const ZIP_STREAM_URIS: StaticStorage = table("_zip_stream_uri", WRAPPER_HANDLE_CAPACITY, ZIP_URI_SLOT_BYTES);
pub const ZIP_STREAM_MODE_LENGTHS: StaticStorage = table("_zip_stream_mode_len", WRAPPER_HANDLE_CAPACITY, WORD);
pub const ZIP_STREAM_MODES: StaticStorage = table("_zip_stream_mode", WRAPPER_HANDLE_CAPACITY, ZIP_MODE_SLOT_BYTES);

/// Declares all per-handle synthetic stream metadata tables.
pub fn append_zip_metadata(out: &mut String, target: &impl crate::data::DataTarget) {
    for storage in [
        ZIP_STREAM_KINDS,
        ZIP_STREAM_URI_LENGTHS,
        ZIP_STREAM_URIS,
        ZIP_STREAM_MODE_LENGTHS,
        ZIP_STREAM_MODES,
    ] {
        storage.append(out, target);
    }
}

/// Loads one synthetic stream kind selected by a checked handle index.
pub fn emit_load_zip_kind(
    emitter: &mut impl Emitter,
    destination: &str,
    index: &str,
    base: &str,
) {
    ZIP_STREAM_KINDS.emit_load_indexed_byte(emitter, destination, index, base);
}

/// Stores one synthetic stream kind selected by a checked handle index.
pub fn emit_store_zip_kind(
    emitter: &mut impl Emitter,
    source: &str,
    index: &str,
    base: &str,
) {
    ZIP_STREAM_KINDS.emit_store_indexed_byte(emitter, source, index, base);
}

/// Addresses one synthetic stream string window selected by a checked handle index.
pub fn emit_zip_string_slot_address(
    emitter: &mut impl Emitter,
    field: ZipMetadataString,
    destination: &str,
    index: &str,
    scratch: &str,
) {
    field.storage().emit_address(emitter, destination);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!(
            "add {destination}, {destination}, {index}, lsl #{}",
            field.shift()
        )),
        Arch::X86_64 => {
            emitter.emit_reg_move(scratch, index);
            emitter.instruction(&format!("shl {scratch}, {}", field.shift()));
            emitter.instruction(&format!("add {destination}, {scratch}"));
        }
    }
}

/// Loads the stored length for one synthetic stream string.
pub fn emit_load_zip_string_length(
    emitter: &mut impl Emitter,
    field: ZipMetadataString,
    destination: &str,
    index: &str,
    base: &str,
) {
    field.length_storage().emit_load_indexed_word(emitter, destination, index, base);
}

/// Stores the copied length for one synthetic stream string.
pub fn emit_store_zip_string_length(
    emitter: &mut impl Emitter,
    field: ZipMetadataString,
    source: &str,
    index: &str,
    base: &str,
) {
    field.length_storage().emit_store_indexed_word(emitter, source, index, base);
}

/// Clears the request's stream side tables after request-owned resources have been closed.
pub fn emit_request_clear(emitter: &mut impl Emitter) {
    for storage in [
        USER_WRAPPERS, USER_WRAPPER_HANDLES, USER_FILTER_REGISTRY, USER_FILTER_INSTANCES,
        DIR_HANDLES, GLOB_HANDLES, POPEN_FILES, ICONV_HANDLES, BZIP2_HANDLES, ZLIB_HANDLES,
        READ_FILTERS, WRITE_FILTERS, EOF_FLAGS, CHUNK_SIZES, CONNECT_HOSTS,
    ] {
        storage.emit_clear(emitter);
    }
}

/// Addresses the host pair at a checked fd. x86_64 consumes the index register.
pub fn emit_connect_host_slot(emitter: &mut impl Emitter, base: &str, index: &str) {
    CONNECT_HOSTS.emit_address(emitter, base);
    emit_connect_host_offset(emitter, base, index);
}

/// Addresses one wrapper or user-filter registration row selected by its checked index.
pub fn emit_registration_entry_address(
    storage: StaticStorage,
    emitter: &mut impl Emitter,
    destination: &str,
    index: &str,
    base: &str,
) {
    storage.emit_address(emitter, base);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!(
            "add {destination}, {base}, {index}, lsl #{REGISTRATION_SHIFT}"
        )),
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {destination}, {index}"));
            emitter.instruction(&format!("shl {destination}, {REGISTRATION_SHIFT}"));
            emitter.instruction(&format!("add {destination}, {base}"));
        }
    }
}

/// Loads one typed word from a materialized registration row.
pub fn emit_load_registration_field(
    emitter: &mut impl Emitter,
    destination: &str,
    entry: &str,
    field: RegistrationField,
) {
    emitter.emit_load_from_address(destination, entry, field.offset());
}

/// Loads one typed word from a per-class user-filter vtable.
pub fn emit_load_user_filter_vtable_field(
    emitter: &mut impl Emitter,
    destination: &str,
    vtable: &str,
    field: UserFilterVtableField,
) {
    emitter.emit_load_from_address(destination, vtable, field.offset());
}

/// Stores one typed word into a materialized registration row.
pub fn emit_store_registration_field(
    emitter: &mut impl Emitter,
    source: &str,
    entry: &str,
    field: RegistrationField,
) {
    emitter.emit_store_to_address(source, entry, field.offset());
}

/// Clears one typed word in a materialized registration row.
pub fn emit_clear_registration_field(
    emitter: &mut impl Emitter,
    entry: &str,
    field: RegistrationField,
) {
    emitter.emit_store_zero_to_address(entry, field.offset());
}

/// Adds the fd offset after the caller materializes the base and reloads the index.
fn emit_connect_host_offset(emitter: &mut impl Emitter, base: &str, index: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {base}, {base}, {index}, lsl #{HOST_PAIR_SHIFT}")); // address the checked descriptor's host pair
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("shl {index}, {HOST_PAIR_SHIFT}"));    // scale the index by the host pair stride
            emitter.instruction(&format!("add {base}, {index}"));               // address the checked descriptor's host pair
        }
    }
}

/// Persists the selected host and publishes its pair using the existing stash frame.
pub fn emit_stash_connect_host(emitter: &mut impl Emitter) {
    emitter.emit_call_label("__rt_str_persist");
    let (base, index) = match emitter.arch() {
        Arch::AArch64 => ("x10", "x9"),
        Arch::X86_64 => ("r10", "r9"),
    };
    CONNECT_HOSTS.emit_address(emitter, base);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("ldr x9, [sp, #0]"),               // reload the descriptor index
        Arch::X86_64 => emitter.instruction("mov r9, QWORD PTR [rbp - 8]"),     // reload the descriptor index
    }
    emit_connect_host_offset(emitter, base, index);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("str x1, [x10, #0]");                           // publish the persisted host pointer
            emitter.instruction("str x2, [x10, #8]");                           // publish the persisted host length
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [r10 + 0], rax");                // publish the persisted host pointer
            emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                // publish the persisted host length
        }
    }
}

/// Detaches a closed fd's string owner and clears the scalar state associated with the fd.
pub fn emit_retire_fd_metadata(emitter: &mut impl Emitter) {
    let fd = emitter.int_result_reg();
    let base = match emitter.arch() {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r9",
    };
    emitter.emit_push_reg(fd);
    emit_connect_host_slot(emitter, base, fd);
    emitter.emit_load_from_address(fd, base, 0);
    emitter.emit_store_zero_to_address(base, 0);
    emitter.emit_store_zero_to_address(base, WORD);
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.emit_pop_reg(fd);

    for table in [READ_FILTERS, WRITE_FILTERS, EOF_FLAGS] {
        table.emit_clear_indexed_byte(emitter, fd, base);
    }
    CHUNK_SIZES.emit_clear_indexed_word(emitter, fd, base);
}

/// Scans for a vacant wrapper slot, leaving the table base and index live.
pub fn emit_find_wrapper_slot(emitter: &mut impl Emitter, scan: &str, found: &str, full: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            USER_WRAPPER_HANDLES.emit_address(emitter, "x10");
            emitter.instruction("mov x12, #0");                                 // start at the first wrapper slot
            emitter.label(scan);
            emitter.instruction(&format!("cmp x12, #{WRAPPER_HANDLE_CAPACITY}")); // check whether the table is full
            emitter.instruction(&format!("b.ge {full}"));                       // let the caller retire the unpublished owner
            emitter.instruction("ldr x13, [x10, x12, lsl #3]");                 // load the wrapper owner
            emitter.instruction(&format!("cbz x13, {found}"));                  // select this vacant slot
            emitter.instruction("add x12, x12, #1");                            // advance one slot
            emitter.instruction(&format!("b {scan}"));                          // examine the next slot
        }
        Arch::X86_64 => {
            USER_WRAPPER_HANDLES.emit_address(emitter, "r10");
            emitter.instruction("xor r12, r12");                                // start at the first wrapper slot
            emitter.label(scan);
            emitter.instruction(&format!("cmp r12, {WRAPPER_HANDLE_CAPACITY}")); // check whether the table is full
            emitter.instruction(&format!("jge {full}"));                        // let the caller retire the unpublished owner
            emitter.instruction("mov r13, QWORD PTR [r10 + r12 * 8]");          // load the wrapper owner
            emitter.instruction("test r13, r13");                               // identify a vacant slot
            emitter.instruction(&format!("jz {found}"));                        // select this vacant slot
            emitter.instruction("inc r12");                                     // advance one slot
            emitter.instruction(&format!("jmp {scan}"));                        // examine the next slot
        }
    }
    emitter.label(found);
}

/// Transfers the wrapper object in the scan's owner register into the selected slot.
pub fn emit_publish_wrapper_owner(emitter: &mut impl Emitter) {
    USER_WRAPPER_HANDLES.emit_store_indexed_word(
        emitter,
        if emitter.arch() == Arch::AArch64 { "x13" } else { "r13" },
        if emitter.arch() == Arch::AArch64 { "x12" } else { "r12" },
        if emitter.arch() == Arch::AArch64 { "x10" } else { "r10" },
    );
}

/// Borrows a wrapper owner and leaves the table address live for an optional detach.
pub fn emit_load_wrapper_owner(
    emitter: &mut impl Emitter, base: &str, index: &str, destination: &str,
) {
    USER_WRAPPER_HANDLES.emit_address(emitter, base);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("ldr {destination}, [{base}, {index}, lsl #3]")), // borrow the selected wrapper object
        Arch::X86_64 => emitter.instruction(&format!("mov {destination}, QWORD PTR [{base} + {index} * 8]")), // borrow the selected wrapper object
    }
}

/// Clears the slot after its owner is moved to the caller's local storage.
pub fn emit_clear_wrapper_slot(emitter: &mut impl Emitter, base: &str, index: &str) {
    USER_WRAPPER_HANDLES.emit_clear_indexed_word(emitter, index, base);
}

/// Loads one user-filter instance selected by its linearized fd and direction index.
pub fn emit_load_filter_instance(
    emitter: &mut impl Emitter,
    destination: &str,
    index: &str,
    base: &str,
) {
    USER_FILTER_INSTANCES.emit_load_indexed_word(emitter, destination, index, base);
}

/// Publishes one user-filter instance selected by its linearized fd and direction index.
pub fn emit_store_filter_instance(
    emitter: &mut impl Emitter,
    source: &str,
    index: &str,
    base: &str,
) {
    USER_FILTER_INSTANCES.emit_store_indexed_word(emitter, source, index, base);
}

/// Detaches one user-filter instance selected by its linearized fd and direction index.
pub fn emit_clear_filter_instance(emitter: &mut impl Emitter, index: &str, base: &str) {
    USER_FILTER_INSTANCES.emit_clear_indexed_word(emitter, index, base);
}

/// Loads both adjacent direction owners and combines their presence into `destination`.
pub fn emit_load_filter_instance_pair_presence(
    emitter: &mut impl Emitter,
    destination: &str,
    second: &str,
    first_index: &str,
    base: &str,
) {
    USER_FILTER_INSTANCES.emit_address(emitter, base);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {base}, {base}, {first_index}, lsl #3"));
            emitter.instruction(&format!("ldp {destination}, {second}, [{base}]"));
            emitter.instruction(&format!("orr {destination}, {destination}, {second}"));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {destination}, QWORD PTR [{base} + {first_index} * 8]"
            ));
            emitter.instruction(&format!(
                "or {destination}, QWORD PTR [{base} + {first_index} * 8 + 8]"
            ));
        }
    }
}
