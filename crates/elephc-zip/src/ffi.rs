//! Purpose:
//! Raw FFI declarations for the pinned libzip 1.11.4 C API used by the `elephc_zip` bridge.
//!
//! Called from:
//! - `crate::archive` for every archive operation.
//!
//! Key details:
//! - The bridge's staticlib never links libzip itself; the final PHP-program link resolves
//!   these symbols from the managed `libzip` native package.
//! - Struct layouts mirror libzip's public `zip.h` for the 64-bit targets Elephc supports
//!   (`time_t` is `i64` on every one of them).

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_uint, c_void};

/// Opaque archive handle.
#[repr(C)]
pub struct zip_t {
    _private: [u8; 0],
}

/// Opaque open-entry handle.
#[repr(C)]
pub struct zip_file_t {
    _private: [u8; 0],
}

/// Opaque source handle.
#[repr(C)]
pub struct zip_source_t {
    _private: [u8; 0],
}

/// Opaque error handle returned by `zip_get_error`/`zip_file_get_error`.
#[repr(C)]
pub struct zip_error_t {
    _private: [u8; 0],
}

/// Opaque libc `FILE` used by `zip_source_filep`.
#[repr(C)]
pub struct FILE {
    _private: [u8; 0],
}

/// Public `struct zip_stat` layout from libzip's `zip.h`.
#[repr(C)]
pub struct zip_stat {
    /// Bit mask of valid fields (`ZIP_STAT_*`).
    pub valid: u64,
    /// Entry name, valid until the next archive mutation.
    pub name: *const c_char,
    /// Entry index inside the archive.
    pub index: u64,
    /// Uncompressed size.
    pub size: u64,
    /// Compressed size.
    pub comp_size: u64,
    /// Modification time.
    pub mtime: i64,
    /// CRC-32 of the uncompressed data.
    pub crc: u32,
    /// Compression method.
    pub comp_method: u16,
    /// Encryption method.
    pub encryption_method: u16,
    /// Reserved flags.
    pub flags: u32,
}

impl zip_stat {
    /// Returns an all-zero stat suitable for `zip_stat_index`/`zip_stat`.
    pub(crate) fn zeroed() -> Self {
        Self {
            valid: 0,
            name: std::ptr::null(),
            index: 0,
            size: 0,
            comp_size: 0,
            mtime: 0,
            crc: 0,
            comp_method: 0,
            encryption_method: 0,
            flags: 0,
        }
    }
}

extern "C" {
    /// Opens or creates an archive; the legacy error code is written to `errorp`.
    pub(crate) fn zip_open(path: *const c_char, flags: c_int, errorp: *mut c_int) -> *mut zip_t;
    /// Writes pending changes and frees the archive on success.
    pub(crate) fn zip_close(za: *mut zip_t) -> c_int;
    /// Frees an archive without writing pending changes.
    pub(crate) fn zip_discard(za: *mut zip_t);
    /// Returns the entry count, including deleted placeholders.
    pub(crate) fn zip_get_num_entries(za: *mut zip_t, flags: u32) -> i64;
    /// Returns one entry's name.
    pub(crate) fn zip_get_name(za: *mut zip_t, index: u64, flags: u32) -> *const c_char;
    /// Locates an entry by name.
    pub(crate) fn zip_name_locate(za: *mut zip_t, name: *const c_char, flags: u32) -> i64;
    /// Reads one entry's stat record by index.
    pub(crate) fn zip_stat_index(za: *mut zip_t, index: u64, flags: u32, st: *mut zip_stat) -> c_int;
    /// Reads one entry's stat record by name.
    pub(crate) fn zip_stat(za: *mut zip_t, name: *const c_char, flags: u32, st: *mut zip_stat) -> c_int;
    /// Returns the archive comment.
    pub(crate) fn zip_get_archive_comment(za: *mut zip_t, lenp: *mut c_int, flags: u32) -> *const c_char;
    /// Sets or removes the archive comment.
    pub(crate) fn zip_set_archive_comment(za: *mut zip_t, comment: *const c_char, len: u16) -> c_int;
    /// Reads one archive flag.
    pub(crate) fn zip_get_archive_flag(za: *mut zip_t, flag: u32, flags: u32) -> c_int;
    /// Sets one archive flag.
    pub(crate) fn zip_set_archive_flag(za: *mut zip_t, flag: u32, value: c_int) -> c_int;
    /// Returns one entry comment.
    pub(crate) fn zip_file_get_comment(za: *mut zip_t, index: u64, lenp: *mut c_uint, flags: u32) -> *const c_char;
    /// Sets or removes one entry comment.
    pub(crate) fn zip_file_set_comment(za: *mut zip_t, index: u64, comment: *const c_char, len: u16, flags: u32) -> c_int;
    /// Sets one entry mtime.
    pub(crate) fn zip_file_set_mtime(za: *mut zip_t, index: u64, mtime: i64, flags: u32) -> c_int;
    /// Sets one entry's external attributes.
    pub(crate) fn zip_file_set_external_attributes(za: *mut zip_t, index: u64, flags: u32, opsys: u8, attr: u32) -> c_int;
    /// Reads one entry's external attributes.
    pub(crate) fn zip_file_get_external_attributes(za: *mut zip_t, index: u64, flags: u32, opsys: *mut u8, attr: *mut u32) -> c_int;
    /// Sets one entry's encryption method and password.
    pub(crate) fn zip_file_set_encryption(za: *mut zip_t, index: u64, method: u16, password: *const c_char) -> c_int;
    /// Sets one entry's compression method.
    pub(crate) fn zip_set_file_compression(za: *mut zip_t, index: u64, method: i32, compflags: u32) -> c_int;
    /// Sets the archive-wide default password used when reading entries.
    pub(crate) fn zip_set_default_password(za: *mut zip_t, password: *const c_char) -> c_int;
    /// Opens one entry for reading.
    pub(crate) fn zip_fopen_index(za: *mut zip_t, index: u64, flags: u32) -> *mut zip_file_t;
    /// Reads uncompressed or raw compressed bytes from an open entry.
    pub(crate) fn zip_fread(zf: *mut zip_file_t, buf: *mut c_void, nbytes: u64) -> i64;
    /// Closes an open entry, returning its final error state.
    pub(crate) fn zip_fclose(zf: *mut zip_file_t) -> c_int;
    /// Returns an open entry's error handle.
    pub(crate) fn zip_file_get_error(zf: *mut zip_file_t) -> *mut zip_error_t;
    /// Returns the zip error code of an error handle.
    pub(crate) fn zip_error_code_zip(err: *mut zip_error_t) -> c_int;
    /// Returns the system error code of an error handle.
    pub(crate) fn zip_error_code_system(err: *mut zip_error_t) -> c_int;
    /// Returns the archive's current error handle.
    pub(crate) fn zip_get_error(za: *mut zip_t) -> *mut zip_error_t;
    /// Returns an error handle's human-readable message.
    pub(crate) fn zip_error_strerror(err: *mut zip_error_t) -> *const c_char;
    /// Releases the cached message of an error handle.
    pub(crate) fn zip_error_fini(err: *mut zip_error_t);
    /// Clears the archive's current error.
    pub(crate) fn zip_error_clear(za: *mut zip_t);
    /// Creates a source reading from a caller-owned memory buffer.
    pub(crate) fn zip_source_buffer(za: *mut zip_t, data: *const c_void, len: u64, freep: c_int) -> *mut zip_source_t;
    /// Creates a source reading a file slice lazily at close time.
    pub(crate) fn zip_source_file(za: *mut zip_t, path: *const c_char, start: u64, length: i64) -> *mut zip_source_t;
    /// Creates a source owning an already-open `FILE *`.
    pub(crate) fn zip_source_filep(za: *mut zip_t, file: *mut FILE, start: u64, length: i64) -> *mut zip_source_t;
    /// Opens a libc file stream.
    pub(crate) fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE;
    /// Closes a libc file stream.
    pub(crate) fn fclose(file: *mut FILE) -> c_int;
    /// Frees an unused source.
    pub(crate) fn zip_source_free(src: *mut zip_source_t);
    /// Adds one entry from a source.
    pub(crate) fn zip_file_add(za: *mut zip_t, name: *const c_char, src: *mut zip_source_t, flags: u32) -> i64;
    /// Replaces one entry's source.
    pub(crate) fn zip_file_replace(za: *mut zip_t, index: u64, src: *mut zip_source_t, flags: u32) -> c_int;
    /// Adds one directory entry.
    pub(crate) fn zip_dir_add(za: *mut zip_t, name: *const c_char, flags: u32) -> i64;
    /// Deletes one entry.
    pub(crate) fn zip_delete(za: *mut zip_t, index: u64) -> c_int;
    /// Renames one entry.
    pub(crate) fn zip_file_rename(za: *mut zip_t, index: u64, name: *const c_char, flags: u32) -> c_int;
    /// Reverts one entry.
    pub(crate) fn zip_unchange(za: *mut zip_t, index: u64) -> c_int;
    /// Reverts all entries.
    pub(crate) fn zip_unchange_all(za: *mut zip_t) -> c_int;
    /// Reverts archive-level changes.
    pub(crate) fn zip_unchange_archive(za: *mut zip_t) -> c_int;
    /// Reports whether a compression method is supported.
    pub(crate) fn zip_compression_method_supported(method: c_int, encode: c_int) -> c_int;
    /// Reports whether an encryption method is supported.
    pub(crate) fn zip_encryption_method_supported(method: u16, encode: c_int) -> c_int;
    /// Returns the linked libzip version string.
    pub(crate) fn zip_libzip_version() -> *const c_char;
}
