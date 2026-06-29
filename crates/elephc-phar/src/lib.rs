//! Purpose:
//! Pure-Rust archive bridge for elephc's `phar://` runtime paths.
//! Extracts and lists native PHAR, tar-based PHAR, and zip-based PHAR entries,
//! and writes or deletes archive entries through a small C ABI so generated
//! assembly does not duplicate archive parsers or manifest writers.
//!
//! Called from:
//! - Compiled PHP program assembly through the `_elephc_phar_extract_url_fn`
//!   and `_elephc_phar_put_entry_fn` / PHAR stream slots.
//! - `src/codegen/builtins/io/phar_stream.rs` for literal compile-time reads.
//! - `cargo test -p elephc-phar` for in-isolation validation.
//!
//! Key details:
//! - Returned FFI pointers reference a process-global buffer and remain valid
//!   until the next `elephc_phar_extract_url` or `elephc_phar_list_entries` call.
//! - Writes preserve the archive family for existing native PHAR, tar, and ZIP
//!   archives. Native PHAR gzip/bzip2 entries and ZIP deflate entries keep their
//!   compression when replaced. ZIP64 archives are read and written (entry counts
//!   over 65535 or sizes/offsets over 4 GiB). Traditional-PKWARE (ZipCrypto)
//!   encrypted entries are read and written, using a password set via
//!   `elephc_phar_set_zip_password` (ZipCrypto is cryptographically weak — kept for
//!   compatibility, not as a real confidentiality mechanism).

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const PHAR_FLAG_GZIP: u32 = 0x0000_1000;
const PHAR_FLAG_BZIP2: u32 = 0x0000_2000;
const PHAR_HDR_SIGNATURE: u32 = 0x0001_0000;
const PHAR_FILE_MODE_0644: u32 = 0x0000_01a4;
const PHAR_SHA1_SIGNATURE_TYPE: u32 = 0x0000_0002;
const PHAR_OPENSSL_SIGNATURE_TYPE: u32 = 0x0000_0010;
const ZIP_METHOD_STORE: u16 = 0;
const ZIP_METHOD_DEFLATE: u16 = 8;
/// ZIP general-purpose flag bit 0: the entry is encrypted (traditional ZipCrypto).
const ZIP_FLAG_ENCRYPTED: u16 = 0x0001;
/// ZIP general-purpose flag bit 3: sizes/CRC are in a trailing data descriptor.
const ZIP_FLAG_DATA_DESCRIPTOR: u16 = 0x0008;
/// ZIP64 extended-information extra-field tag.
const ZIP64_EXTRA_TAG: u16 = 0x0001;
/// 32-bit field value meaning "real value is in the ZIP64 extra field / EOCD64".
const ZIP32_SENTINEL: u32 = 0xFFFF_FFFF;
/// 16-bit entry-count field value meaning "real count is in the EOCD64".
const ZIP16_SENTINEL: u16 = 0xFFFF;
const PHAR_WRITE_FD_BASE: usize = 0x5000_0000;
const PHAR_WRITE_STREAM_LIMIT: usize = 32;

static EXTRACT_BUFFER: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
static WRITE_STREAMS: OnceLock<Mutex<Vec<Option<WriteStream>>>> = OnceLock::new();
thread_local! {
    /// Password used to read and write traditional-PKWARE (ZipCrypto) encrypted ZIP
    /// entries, set through [`elephc_phar_set_zip_password`]; `None` until provided.
    /// When set, zip phars are written with their entries encrypted. Thread-local:
    /// it is set and consumed on the same (single) runtime thread, which also keeps
    /// parallel unit tests from clobbering each other's password state.
    static ZIP_PASSWORD: std::cell::RefCell<Option<Vec<u8>>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PharCompression {
    None,
    Gzip,
    Bzip2,
}

#[derive(Clone)]
struct ArchiveEntry {
    name: Vec<u8>,
    payload: Vec<u8>,
    compression: PharCompression,
    /// PHP-`serialize()`d per-file metadata blob (empty when the entry has none).
    /// Stored in the native manifest's per-entry metadata field, a tar
    /// `.phar/.metadata/<path>/.metadata.bin` side entry, or a zip central-directory
    /// file comment, depending on the archive family.
    metadata: Vec<u8>,
}

#[derive(Clone, Copy)]
enum ArchiveFormat {
    NativePhar,
    Tar,
    Zip,
}

/// Internal entry name holding a tar/zip-based phar's executable stub.
const PHAR_STUB_ENTRY: &[u8] = b".phar/stub.php";
/// Internal entry name holding a tar/zip-based phar's serialized global metadata.
const PHAR_METADATA_ENTRY: &[u8] = b".phar/.metadata.bin";
/// Internal entry name holding a tar/zip-based phar's signature. Its payload is
/// `LE32(sig_flag) ++ LE32(sig_len) ++ signature`, and it is always the archive's
/// last entry so the signed range is everything that precedes it.
const PHAR_SIGNATURE_ENTRY: &[u8] = b".phar/signature.bin";
/// Prefix of the tar side entry holding one file's serialized metadata. The full
/// name is `.phar/.metadata/<entry-path>/.metadata.bin` (matching php-src).
const PHAR_FILE_METADATA_PREFIX: &[u8] = b".phar/.metadata/";
/// Suffix of the tar per-file metadata side entry (see [`PHAR_FILE_METADATA_PREFIX`]).
const PHAR_FILE_METADATA_SUFFIX: &[u8] = b"/.metadata.bin";
/// Default native-PHAR stub emitted when no custom stub has been set.
const PHAR_DEFAULT_STUB: &[u8] = b"<?php __HALT_COMPILER(); ?>\r\n";

/// A parsed archive plus its archive-level global metadata and stub.
///
/// `metadata` holds the PHP-`serialize()`d global metadata blob (empty when none);
/// `stub` holds the executable stub bytes (empty when none/default). Both are
/// preserved across read-modify-write cycles and re-emitted by [`build_archive`].
#[derive(Clone)]
struct Archive {
    entries: Vec<ArchiveEntry>,
    format: ArchiveFormat,
    metadata: Vec<u8>,
    stub: Vec<u8>,
}

/// Returns true for the reserved `.phar/*` control entries that phars hide from
/// their public entry listing (stub, metadata, alias, signature, per-file metadata).
fn is_phar_control_entry(name: &[u8]) -> bool {
    name.starts_with(b".phar/")
}

enum WriteStreamTarget {
    Entry { archive: Vec<u8>, entry: Vec<u8> },
    Url(Vec<u8>),
}

struct WriteStream {
    target: WriteStreamTarget,
    payload: Vec<u8>,
}

/// Extracts a `phar://archive/entry` URL into bytes.
///
/// The archive portion is found by scanning slash-delimited prefixes until one
/// names an existing file. This matches PHP's archive-boundary behavior while
/// also supporting `.phar`, `.tar`, and `.zip` suffixes without hardcoding an
/// extension list.
pub fn extract_url_bytes(url: &[u8]) -> Option<Vec<u8>> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry) = split_archive_entry(rest)?;
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let archive = std::fs::read(archive_path).ok()?;
    extract_entry_bytes(&archive, entry)
}

/// Extracts `entry` from already-loaded archive bytes.
///
/// Native PHAR is tried first because it has an explicit manifest and may have
/// arbitrary stubs before the payload. Plain ZIP and TAR containers are then
/// tried by signature/layout.
pub fn extract_entry_bytes(archive: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    // Whole-archive gzip/bzip2 wrappers are decoded transparently before extraction.
    if archive.starts_with(b"\x1f\x8b") {
        return extract_entry_bytes(&decompress_gzip_stream(archive)?, entry);
    }
    if archive.starts_with(b"BZh") {
        return extract_entry_bytes(&decompress_bzip2_stream(archive)?, entry);
    }
    parse_native_phar_entry(archive, entry)
        .or_else(|| parse_zip_entry(archive, entry))
        .or_else(|| parse_tar_entry(archive, entry))
}

/// Serializes every supported entry name from an archive on disk.
///
/// The output is a packed sequence of `u64 little-endian length` followed by
/// raw entry-name bytes. This keeps the C ABI simple while letting generated
/// code build a PHP string array without knowing the archive format.
pub fn entry_names_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let archive = std::fs::read(archive_path).ok()?;
    let (entries, _) = parse_archive_entries(&archive)?;
    let mut out = Vec::new();
    for entry in entries {
        let name_len = u64::try_from(entry.name.len()).ok()?;
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&entry.name);
    }
    Some(out)
}

/// Inserts or replaces one entry in an archive on disk.
///
/// Missing archives are created as native PHAR unless the path extension is
/// `.tar` or `.zip`. Existing native PHAR, tar, and ZIP archives are read,
/// decoded, updated, and rewritten in their original archive family.
pub fn put_entry_bytes(
    archive_path: &[u8],
    entry_name: &[u8],
    payload: &[u8],
) -> Option<usize> {
    if entry_name.is_empty() {
        return None;
    }
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(archive_path);
    let mut archive = if path.exists() {
        let bytes = std::fs::read(path).ok()?;
        parse_archive(&bytes)?
    } else {
        Archive {
            entries: Vec::new(),
            format: format_for_new_archive_path(path),
            metadata: Vec::new(),
            stub: Vec::new(),
        }
    };
    upsert_entry(&mut archive.entries, entry_name, payload);
    let out = build_archive_value(&archive)?;
    std::fs::write(path, out).ok()?;
    Some(payload.len())
}

/// Inserts or replaces one uncompressed entry described by a full `phar://` URL.
///
/// The write splitter mirrors codegen's literal write handling: prefer the
/// first `.phar/` boundary when present, otherwise use the final slash as the
/// archive/entry separator.
pub fn put_url_bytes(url: &[u8], payload: &[u8]) -> Option<usize> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry_name) = split_write_url_entry(rest)?;
    put_entry_bytes(archive_path, entry_name, payload)
}

/// Removes one entry from an archive on disk.
///
/// Existing native PHAR, tar, and ZIP archives are decoded and rewritten in
/// their original archive family. Missing archives or missing entries return
/// `None`, matching PHP's false-result path for failed `unlink()`.
pub fn delete_entry_bytes(archive_path: &[u8], entry_name: &[u8]) -> Option<()> {
    if entry_name.is_empty() {
        return None;
    }
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(archive_path);
    let bytes = std::fs::read(path).ok()?;
    let mut archive = parse_archive(&bytes)?;
    remove_entry(&mut archive.entries, entry_name)?;
    let out = build_archive_value(&archive)?;
    std::fs::write(path, out).ok()?;
    Some(())
}

/// Removes one entry described by a full `phar://` URL.
pub fn delete_url_bytes(url: &[u8]) -> Option<()> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry_name) = split_write_url_entry(rest)?;
    delete_entry_bytes(archive_path, entry_name)
}

/// Updates all supported entry compression flags in an archive on disk.
///
/// Compression codes follow PHP's `Phar::NONE`, `Phar::GZ`, and `Phar::BZ2`
/// constants. Native PHAR supports gzip and bzip2 entry payloads, ZIP supports
/// stored and deflated entries, and tar returns `None` because compression is
/// archive-wide rather than per-entry.
pub fn set_archive_compression(archive_path: &[u8], compression_code: usize) -> Option<()> {
    let compression = compression_from_php_constant(compression_code)?;
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(archive_path);
    let bytes = std::fs::read(path).ok()?;
    let mut archive = parse_archive(&bytes)?;
    if matches!(archive.format, ArchiveFormat::Tar) {
        return None;
    }
    if matches!(archive.format, ArchiveFormat::Zip)
        && matches!(compression, PharCompression::Bzip2)
    {
        return None;
    }
    for entry in &mut archive.entries {
        entry.compression = compression;
    }
    let out = build_archive_value(&archive)?;
    std::fs::write(path, out).ok()?;
    Some(())
}

/// Reads an archive's serialized global metadata blob (empty when unset).
fn get_metadata_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let path = std::str::from_utf8(archive_path).ok()?;
    let bytes = std::fs::read(path).ok()?;
    Some(parse_archive(&bytes)?.metadata)
}

/// Reads an archive's stub bytes (empty when unset / default).
fn get_stub_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let path = std::str::from_utf8(archive_path).ok()?;
    let bytes = std::fs::read(path).ok()?;
    Some(parse_archive(&bytes)?.stub)
}

/// Sets an archive's global metadata, preserving all entries and the stub.
///
/// Creates the archive (format chosen by extension) when it does not yet exist.
fn set_metadata_bytes(archive_path: &[u8], metadata: &[u8]) -> Option<()> {
    let path_str = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(path_str);
    let mut archive = read_or_new_archive(path)?;
    archive.metadata = metadata.to_vec();
    std::fs::write(path, build_archive_value(&archive)?).ok()?;
    Some(())
}

/// Sets an archive's stub, preserving all entries and global metadata.
///
/// The stub must contain `__HALT_COMPILER();` (matching PHP); creates the archive
/// (format chosen by extension) when it does not yet exist.
fn set_stub_bytes(archive_path: &[u8], stub: &[u8]) -> Option<()> {
    if find_subslice(stub, b"__HALT_COMPILER();").is_none() {
        return None;
    }
    let path_str = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(path_str);
    let mut archive = read_or_new_archive(path)?;
    archive.stub = stub.to_vec();
    std::fs::write(path, build_archive_value(&archive)?).ok()?;
    Some(())
}

/// Parses an existing archive, or builds an empty one whose format follows the path.
fn read_or_new_archive(path: &std::path::Path) -> Option<Archive> {
    if path.exists() {
        parse_archive(&std::fs::read(path).ok()?)
    } else {
        Some(Archive {
            entries: Vec::new(),
            format: format_for_new_archive_path(path),
            metadata: Vec::new(),
            stub: Vec::new(),
        })
    }
}

/// C ABI wrapper around [`extract_url_bytes`].
///
/// Returns a pointer to a stable process-global buffer and writes the byte
/// length into `out_len`. Returns null and writes zero on any failure.
///
/// # Safety
/// `url_ptr` must be valid for `url_len` bytes unless `url_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_extract_url(
    url_ptr: *const u8,
    url_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| extract_url_bytes(slice(url_ptr, url_len))) {
        Ok(Some(bytes)) => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`entry_names_bytes`].
///
/// Returns a pointer to the serialized entry-name buffer and writes its byte
/// length into `out_len`. Returns null and writes zero when the archive cannot
/// be read or parsed.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_list_entries(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| entry_names_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`put_entry_bytes`].
///
/// Returns the written payload length on success and `usize::MAX` on failure.
/// The archive is always a native PHAR after a successful write.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `entry_ptr` must not describe an empty entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_put_entry(
    archive_ptr: *const u8,
    archive_len: usize,
    entry_ptr: *const u8,
    entry_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        put_entry_bytes(
            slice(archive_ptr, archive_len),
            slice(entry_ptr, entry_len),
            slice(data_ptr, data_len),
        )
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// C ABI wrapper around [`put_url_bytes`].
///
/// Returns the written payload length on success and `usize::MAX` on failure.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `url_ptr` must point to a complete `phar://archive/entry` URL.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_put_url(
    url_ptr: *const u8,
    url_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        put_url_bytes(slice(url_ptr, url_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// C ABI wrapper around [`delete_url_bytes`].
///
/// Returns `1` when the entry was removed and the archive was rewritten, or `0`
/// when the URL is invalid, the archive cannot be parsed, or the entry is absent.
///
/// # Safety
/// `url_ptr` must be valid for `url_len` bytes unless `url_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_delete_url(
    url_ptr: *const u8,
    url_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| delete_url_bytes(slice(url_ptr, url_len)));
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`set_zip_password`].
///
/// Sets the password used to read and write traditional-PKWARE (ZipCrypto)
/// encrypted ZIP entries; an empty password clears it. Always returns `1`.
///
/// # Safety
/// `password_ptr` must be valid for `password_len` bytes unless `password_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_zip_password(
    password_ptr: *const u8,
    password_len: usize,
) -> usize {
    let _ = std::panic::catch_unwind(|| set_zip_password(slice(password_ptr, password_len)));
    1
}

/// C ABI wrapper around [`set_archive_compression`].
///
/// Returns `1` when the native PHAR archive was rewritten, or `0` for invalid
/// paths, unsupported archive families, or unsupported compression constants.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_compression(
    path_ptr: *const u8,
    path_len: usize,
    compression_code: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_archive_compression(slice(path_ptr, path_len), compression_code)
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`get_metadata_bytes`].
///
/// Returns a pointer to the serialized global metadata buffer and writes its byte
/// length into `out_len`. Returns null and writes zero when there is no metadata or
/// the archive cannot be read.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_metadata(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| get_metadata_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) if !bytes.is_empty() => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`get_stub_bytes`].
///
/// Returns a pointer to the stub buffer and writes its byte length into `out_len`.
/// Returns null and writes zero when there is no stub or the archive cannot be read.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_stub(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| get_stub_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) if !bytes.is_empty() => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`set_metadata_bytes`].
///
/// Returns `1` when the archive was rewritten with the new global metadata, or `0`
/// on any failure.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_metadata(
    path_ptr: *const u8,
    path_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_metadata_bytes(slice(path_ptr, path_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`set_stub_bytes`].
///
/// Returns `1` when the archive was rewritten with the new stub, or `0` on any
/// failure (including a stub missing the `__HALT_COMPILER();` marker).
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_stub(
    path_ptr: *const u8,
    path_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_stub_bytes(slice(path_ptr, path_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`get_file_metadata_url`].
///
/// Takes a `phar://archive/entry` URL and returns a pointer to that entry's
/// serialized metadata, writing its byte length into `out_len`. Returns null and
/// writes zero when the entry has no metadata, the entry is absent, or the archive
/// cannot be read.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_file_metadata(
    url_ptr: *const u8,
    url_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    let result = std::panic::catch_unwind(|| get_file_metadata_url(slice(url_ptr, url_len)));
    match result {
        Ok(Some(bytes)) if !bytes.is_empty() => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`set_file_metadata_url`].
///
/// Takes a `phar://archive/entry` URL and serialized metadata, rewriting the archive
/// so the entry carries it (an empty `data` clears it). Returns `1` on success, or
/// `0` on any failure including a missing entry.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_file_metadata(
    url_ptr: *const u8,
    url_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_file_metadata_url(slice(url_ptr, url_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`gzip_archive`] — whole-archive gzip compression.
///
/// Returns a pointer to the written destination path and writes its length into
/// `out_len`; returns null and writes zero on failure.
///
/// # Safety
/// `src` must be valid for `src_len` unless zero; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_gzip_archive(
    src_ptr: *const u8,
    src_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| gzip_archive(slice(src_ptr, src_len))),
        out_len,
    )
}

/// C ABI wrapper around [`bzip2_archive`] — whole-archive bzip2 compression.
///
/// Returns a pointer to the written destination path and writes its length into
/// `out_len`; returns null and writes zero on failure.
///
/// # Safety
/// `src` must be valid for `src_len` unless zero; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_bzip2_archive(
    src_ptr: *const u8,
    src_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| bzip2_archive(slice(src_ptr, src_len))),
        out_len,
    )
}

/// C ABI wrapper around [`decompress_archive`] — whole-archive decompression.
///
/// Returns a pointer to the written destination path and writes its length into
/// `out_len`; returns null and writes zero on failure (including an uncompressed src).
///
/// # Safety
/// `src` must be valid for `src_len` unless zero; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_decompress_archive(
    src_ptr: *const u8,
    src_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| decompress_archive(slice(src_ptr, src_len))),
        out_len,
    )
}

/// Shared result handling for the archive (de)compression bridges: publishes a
/// non-empty destination path, or returns null + zero length on failure.
fn publish_archive_path_result(
    result: std::thread::Result<Option<Vec<u8>>>,
    out_len: *mut usize,
) -> *const u8 {
    match result {
        Ok(Some(path)) if !path.is_empty() => publish_result(path, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`sign_archive_openssl`] — RSA-SHA1 (OpenSSL) PHAR signing.
///
/// Returns `1` when the archive was re-signed, `0` on any failure (bad key, unreadable
/// archive).
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_sign_openssl(
    path_ptr: *const u8,
    path_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        sign_archive_openssl(slice(path_ptr, path_len), slice(key_ptr, key_len))
    });
    usize::from(matches!(result, Ok(Some(_))))
}

/// C ABI wrapper around [`sign_archive_hash`] — MD5/SHA1/SHA256/SHA512 PHAR signing.
///
/// Returns `1` when the archive was re-signed, `0` on any failure or unknown `algo`.
///
/// # Safety
/// `path` must be valid for `path_len` unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_sign_hash(
    path_ptr: *const u8,
    path_len: usize,
    algo: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        sign_archive_hash(slice(path_ptr, path_len), algo as u32)
    });
    usize::from(matches!(result, Ok(Some(()))))
}

/// C ABI wrapper around [`signature_hash_hex`] — `Phar::getSignature()['hash']`.
///
/// Returns the uppercase-hex signature/digest pointer and writes its length into
/// `out_len`; returns null + zero on failure.
///
/// # Safety
/// `path` must be valid for `path_len`; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_signature_hash(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| signature_hash_hex(slice(path_ptr, path_len))),
        out_len,
    )
}

/// C ABI wrapper around [`signature_type_name`] — `Phar::getSignature()['hash_type']`.
///
/// Returns the type-name pointer and writes its length into `out_len`; returns null +
/// zero on failure.
///
/// # Safety
/// `path` must be valid for `path_len`; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_signature_type(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| signature_type_name(slice(path_ptr, path_len))),
        out_len,
    )
}

/// C ABI wrapper that opens a buffered write stream for a literal PHAR entry.
///
/// Returns a synthetic descriptor in the `0x50000000..0x50000020` range, or
/// `usize::MAX` when no stream slot is available or the target is invalid.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `entry_ptr` must not describe an empty entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_stream_open_entry(
    archive_ptr: *const u8,
    archive_len: usize,
    entry_ptr: *const u8,
    entry_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        let entry = slice(entry_ptr, entry_len);
        if entry.is_empty() {
            return None;
        }
        allocate_write_stream(WriteStream {
            target: WriteStreamTarget::Entry {
                archive: slice(archive_ptr, archive_len).to_vec(),
                entry: entry.to_vec(),
            },
            payload: Vec::new(),
        })
    });
    match result {
        Ok(Some(fd)) => fd,
        _ => usize::MAX,
    }
}

/// C ABI wrapper that opens a buffered write stream for a runtime PHAR URL.
///
/// Returns a synthetic descriptor in the `0x50000000..0x50000020` range, or
/// `usize::MAX` when no stream slot is available or the URL is invalid.
///
/// # Safety
/// `url_ptr` must be valid for `url_len` bytes unless `url_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_stream_open_url(
    url_ptr: *const u8,
    url_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        let url = slice(url_ptr, url_len);
        if !url.starts_with(b"phar://") {
            return None;
        }
        allocate_write_stream(WriteStream {
            target: WriteStreamTarget::Url(url.to_vec()),
            payload: Vec::new(),
        })
    });
    match result {
        Ok(Some(fd)) => fd,
        _ => usize::MAX,
    }
}

/// C ABI wrapper that appends bytes to an open PHAR write stream.
///
/// Returns the number of consumed bytes on success and `usize::MAX` when `fd`
/// does not name an open PHAR write stream.
///
/// # Safety
/// `data_ptr` must be valid for `data_len` bytes unless `data_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_stream_append(
    fd: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        append_write_stream(fd, slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// C ABI wrapper that finalizes and closes an open PHAR write stream.
///
/// Returns `1` on success and `0` on failure. The stream slot is released before
/// the archive write is attempted, matching fclose-style one-shot finalization.
#[no_mangle]
pub extern "C" fn elephc_phar_stream_finalize(fd: usize) -> usize {
    let result = std::panic::catch_unwind(|| finalize_write_stream(fd));
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// Builds a byte slice from a C pointer and byte length.
///
/// A zero length never dereferences the pointer, so null plus zero is accepted.
unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Stores extracted bytes in the process-global result buffer and returns its pointer.
fn publish_result(bytes: Vec<u8>, out_len: *mut usize) -> *const u8 {
    let mut buffer = EXTRACT_BUFFER
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("elephc_phar extract buffer poisoned");
    buffer.clear();
    buffer.extend_from_slice(&bytes);
    write_len(out_len, buffer.len());
    if buffer.is_empty() {
        b"".as_ptr()
    } else {
        buffer.as_ptr()
    }
}

/// Returns the process-global table for buffered PHAR write streams.
fn write_streams() -> &'static Mutex<Vec<Option<WriteStream>>> {
    WRITE_STREAMS.get_or_init(|| {
        let mut streams = Vec::with_capacity(PHAR_WRITE_STREAM_LIMIT);
        streams.resize_with(PHAR_WRITE_STREAM_LIMIT, || None);
        Mutex::new(streams)
    })
}

/// Allocates a write-stream slot and returns its synthetic descriptor.
fn allocate_write_stream(stream: WriteStream) -> Option<usize> {
    let mut streams = write_streams().lock().ok()?;
    for (slot, current) in streams.iter_mut().enumerate() {
        if current.is_none() {
            *current = Some(stream);
            return Some(PHAR_WRITE_FD_BASE + slot);
        }
    }
    None
}

/// Converts a synthetic PHAR descriptor into a write-stream slot index.
fn write_stream_slot(fd: usize) -> Option<usize> {
    let slot = fd.checked_sub(PHAR_WRITE_FD_BASE)?;
    (slot < PHAR_WRITE_STREAM_LIMIT).then_some(slot)
}

/// Appends payload bytes to an open write stream.
fn append_write_stream(fd: usize, data: &[u8]) -> Option<usize> {
    let slot = write_stream_slot(fd)?;
    let mut streams = write_streams().lock().ok()?;
    let stream = streams.get_mut(slot)?.as_mut()?;
    stream.payload.extend_from_slice(data);
    Some(data.len())
}

/// Finalizes one open write stream and writes its target archive.
fn finalize_write_stream(fd: usize) -> Option<()> {
    let slot = write_stream_slot(fd)?;
    let stream = {
        let mut streams = write_streams().lock().ok()?;
        streams.get_mut(slot)?.take()?
    };
    match stream.target {
        WriteStreamTarget::Entry { archive, entry } => {
            put_entry_bytes(&archive, &entry, &stream.payload)?;
        }
        WriteStreamTarget::Url(url) => {
            put_url_bytes(&url, &stream.payload)?;
        }
    }
    Some(())
}

/// Writes an output length through the optional C pointer.
fn write_len(out_len: *mut usize, len: usize) {
    if !out_len.is_null() {
        unsafe {
            *out_len = len;
        }
    }
}

/// Splits `phar://` URL body bytes into an existing archive path and inner entry name.
fn split_archive_entry(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    for (i, &byte) in rest.iter().enumerate() {
        if byte != b'/' || i == 0 || i + 1 >= rest.len() {
            continue;
        }
        let candidate = std::str::from_utf8(&rest[..i]).ok()?;
        if std::path::Path::new(candidate).is_file() {
            return Some((&rest[..i], &rest[i + 1..]));
        }
    }
    None
}

/// Splits `phar://` URL body bytes for writes, including missing archives.
fn split_write_url_entry(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    for suffix in [b".phar/".as_slice(), b".tar/".as_slice(), b".zip/".as_slice()] {
        if let Some(idx) = find_subslice(rest, suffix) {
            let split = idx.checked_add(suffix.len().checked_sub(1)?)?;
            return Some((rest.get(..split)?, rest.get(split + 1..)?));
        }
    }
    let idx = rest.iter().rposition(|&byte| byte == b'/')?;
    if idx == 0 || idx + 1 >= rest.len() {
        return None;
    }
    Some((rest.get(..idx)?, rest.get(idx + 1..)?))
}

/// Parses archive bytes into a full [`Archive`] (entries plus global metadata/stub).
///
/// Dispatch is by container signature rather than try-each-and-fallback: tar/zip-based
/// phars embed a `.phar/stub.php` containing `__HALT_COMPILER();`, so a native-first
/// scan would mistake them for native PHARs. ZIP starts with `PK\x03\x04` (or
/// `PK\x05\x06` when empty); TAR carries the ustar magic at offset 257; everything
/// else (a `<?php` stub) is a native PHAR.
fn parse_archive(data: &[u8]) -> Option<Archive> {
    // A whole-archive gzip/bzip2 wrapper (e.g. `.tar.gz` / `.tar.bz2`) is decoded
    // transparently, then the inner archive is parsed normally.
    if data.starts_with(b"\x1f\x8b") {
        return parse_archive(&decompress_gzip_stream(data)?);
    }
    if data.starts_with(b"BZh") {
        return parse_archive(&decompress_bzip2_stream(data)?);
    }
    if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
        parse_zip_archive(data)
    } else if data.get(257..262) == Some(b"ustar") {
        parse_tar_archive(data)
    } else {
        parse_native_phar_archive(data)
    }
}

/// Decompresses a whole gzip (`.gz`) stream into its plain bytes.
fn decompress_gzip_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut decoder = flate2::read::GzDecoder::new(data);
    std::io::Read::read_to_end(&mut decoder, &mut out).ok()?;
    Some(out)
}

/// Decompresses a whole bzip2 (`.bz2`) stream into its plain bytes.
fn decompress_bzip2_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut decoder = bzip2_rs::DecoderReader::new(data);
    std::io::Read::read_to_end(&mut decoder, &mut out).ok()?;
    Some(out)
}

/// Returns the plain (uncompressed) archive bytes, stripping a whole-archive gzip or
/// bzip2 wrapper when present so a recompress operates on the canonical archive.
fn uncompressed_archive_bytes(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.starts_with(b"\x1f\x8b") {
        decompress_gzip_stream(raw)
    } else if raw.starts_with(b"BZh") {
        decompress_bzip2_stream(raw)
    } else {
        Some(raw.to_vec())
    }
}

/// Returns the destination path for compressing `src`: any existing `.gz`/`.bz2`
/// suffix is stripped, then `.<new_ext>` is appended (e.g. `foo.tar` → `foo.tar.gz`).
fn compression_dest_path(src: &[u8], new_ext: &str) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(src).ok()?;
    let base = s
        .strip_suffix(".gz")
        .or_else(|| s.strip_suffix(".bz2"))
        .unwrap_or(s);
    Some(format!("{base}.{new_ext}").into_bytes())
}

/// Reads `src`, gzip-wraps its plain archive bytes, writes them to `<base>.gz`, and
/// returns that destination path (PHP `PharData::compress(Phar::GZ)`).
fn gzip_archive(src: &[u8]) -> Option<Vec<u8>> {
    let plain = uncompressed_archive_bytes(&read_path(src)?)?;
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &plain).ok()?;
    let dest = compression_dest_path(src, "gz")?;
    write_path(&dest, &encoder.finish().ok()?)?;
    Some(dest)
}

/// Reads `src`, bzip2-wraps its plain archive bytes, writes them to `<base>.bz2`, and
/// returns that destination path (PHP `PharData::compress(Phar::BZ2)`).
fn bzip2_archive(src: &[u8]) -> Option<Vec<u8>> {
    let plain = uncompressed_archive_bytes(&read_path(src)?)?;
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    std::io::Write::write_all(&mut encoder, &plain).ok()?;
    let dest = compression_dest_path(src, "bz2")?;
    write_path(&dest, &encoder.finish().ok()?)?;
    Some(dest)
}

/// Reads a whole-archive-compressed `src` (a `.gz`/`.bz2` path), writes its plain
/// bytes to the path with that suffix removed, and returns that destination path
/// (PHP `PharData::decompress()`). Fails when `src` carries no compression suffix.
fn decompress_archive(src: &[u8]) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(src).ok()?;
    let dest = s
        .strip_suffix(".gz")
        .or_else(|| s.strip_suffix(".bz2"))?
        .as_bytes()
        .to_vec();
    write_path(&dest, &uncompressed_archive_bytes(&read_path(src)?)?)?;
    Some(dest)
}

/// Reads a filesystem path given as UTF-8 bytes.
fn read_path(path: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(std::str::from_utf8(path).ok()?)).ok()
}

/// Writes `bytes` to a filesystem path given as UTF-8 bytes.
fn write_path(path: &[u8], bytes: &[u8]) -> Option<()> {
    std::fs::write(std::path::Path::new(std::str::from_utf8(path).ok()?), bytes).ok()
}

/// Parses archive bytes into decoded entries and reports the archive family.
fn parse_archive_entries(data: &[u8]) -> Option<(Vec<ArchiveEntry>, ArchiveFormat)> {
    parse_archive(data).map(|archive| (archive.entries, archive.format))
}

/// Selects the archive family for a missing output path.
fn format_for_new_archive_path(path: &std::path::Path) -> ArchiveFormat {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("tar") => ArchiveFormat::Tar,
        Some(ext) if ext.eq_ignore_ascii_case("zip") => ArchiveFormat::Zip,
        _ => ArchiveFormat::NativePhar,
    }
}

/// Builds an archive in the selected output family.
fn build_archive(
    entries: &[ArchiveEntry],
    format: ArchiveFormat,
    metadata: &[u8],
    stub: &[u8],
) -> Option<Vec<u8>> {
    match format {
        ArchiveFormat::NativePhar => build_native_phar_archive(entries, metadata, stub),
        ArchiveFormat::Tar => build_tar_archive(entries, metadata, stub),
        ArchiveFormat::Zip => build_zip_archive(entries, metadata, stub),
    }
}

/// Rebuilds an [`Archive`] into serialized bytes, preserving its metadata and stub.
fn build_archive_value(archive: &Archive) -> Option<Vec<u8>> {
    build_archive(
        &archive.entries,
        archive.format,
        &archive.metadata,
        &archive.stub,
    )
}

/// Parses a native PHAR archive and returns a decoded entry payload.
fn parse_native_phar_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    parse_native_phar_archive(data)?
        .entries
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a native PHAR archive into entries plus its global metadata and stub.
///
/// The stub is the byte prefix up to and including the `__HALT_COMPILER();` marker
/// (and any trailing ` ?>\r\n`); the global metadata is the manifest's metadata field.
fn parse_native_phar_archive(data: &[u8]) -> Option<Archive> {
    let halt = b"__HALT_COMPILER();";
    let halt_idx = find_subslice(data, halt)?;
    let mut p = halt_idx + halt.len();
    for &ch in &[b' ', b'?', b'>', b'\r', b'\n'] {
        if data.get(p) == Some(&ch) {
            p += 1;
        }
    }

    let manifest_start = p;
    let stub = data.get(..manifest_start)?.to_vec();
    let manifest_len = le32(data, manifest_start)? as usize;
    let data_section = manifest_start.checked_add(4)?.checked_add(manifest_len)?;
    let num_files = le32(data, manifest_start + 4)?;
    let mut q = manifest_start + 8 + 2 + 4;
    let alias_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(alias_len)?;
    let meta_len = le32(data, q)? as usize;
    q = q.checked_add(4)?;
    let metadata = data.get(q..q.checked_add(meta_len)?)?.to_vec();
    q = q.checked_add(meta_len)?;

    let mut data_offset = 0usize;
    let mut entries = Vec::with_capacity(num_files as usize);
    for _ in 0..num_files {
        let name_len = le32(data, q)? as usize;
        q = q.checked_add(4)?;
        let name = data.get(q..q.checked_add(name_len)?)?;
        q = q.checked_add(name_len)?;
        let uncompressed = le32(data, q)? as usize;
        q = q.checked_add(4)?;
        q = q.checked_add(4)?;
        let compressed = le32(data, q)? as usize;
        q = q.checked_add(4)?;
        q = q.checked_add(4)?;
        let flags = le32(data, q)?;
        q = q.checked_add(4)?;
        let entry_meta_len = le32(data, q)? as usize;
        q = q.checked_add(4)?;
        let entry_metadata = data.get(q..q.checked_add(entry_meta_len)?)?.to_vec();
        q = q.checked_add(entry_meta_len)?;

        let start = data_section.checked_add(data_offset)?;
        let stored = data.get(start..start.checked_add(compressed)?)?;
        let payload = decode_phar_payload(stored, flags, uncompressed)?;
        entries.push(ArchiveEntry {
            name: name.to_vec(),
            payload,
            compression: phar_compression_from_flags(flags),
            metadata: entry_metadata,
        });
        data_offset = data_offset.checked_add(compressed)?;
    }
    Some(Archive {
        entries,
        format: ArchiveFormat::NativePhar,
        metadata,
        stub,
    })
}

/// Extracts the PHAR compression mode from per-entry flags.
fn phar_compression_from_flags(flags: u32) -> PharCompression {
    if flags & PHAR_FLAG_GZIP != 0 {
        PharCompression::Gzip
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        PharCompression::Bzip2
    } else {
        PharCompression::None
    }
}

/// Decodes a native PHAR entry payload according to its per-entry flags.
fn decode_phar_payload(stored: &[u8], flags: u32, uncompressed: usize) -> Option<Vec<u8>> {
    if flags & PHAR_FLAG_GZIP != 0 {
        let mut out = Vec::with_capacity(uncompressed);
        let mut decoder = flate2::read::DeflateDecoder::new(stored);
        decoder.read_to_end(&mut out).ok()?;
        (out.len() == uncompressed).then_some(out)
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        let mut out = Vec::with_capacity(uncompressed);
        let mut decoder = bzip2_rs::DecoderReader::new(stored);
        decoder.read_to_end(&mut out).ok()?;
        (out.len() == uncompressed).then_some(out)
    } else {
        Some(stored.to_vec())
    }
}

/// Inserts `payload` under `entry_name`, preserving compression for replacements.
fn upsert_entry(entries: &mut Vec<ArchiveEntry>, entry_name: &[u8], payload: &[u8]) {
    if let Some(existing) = entries.iter_mut().find(|entry| entry.name == entry_name) {
        existing.payload.clear();
        existing.payload.extend_from_slice(payload);
    } else {
        entries.push(ArchiveEntry {
            name: entry_name.to_vec(),
            payload: payload.to_vec(),
            compression: PharCompression::None,
            metadata: Vec::new(),
        });
    }
}

/// Returns the serialized per-file metadata for `entry_name`, or `None` if the
/// archive cannot be read or has no such entry.
fn get_file_metadata_bytes(archive_path: &[u8], entry_name: &[u8]) -> Option<Vec<u8>> {
    let path = std::path::Path::new(std::str::from_utf8(archive_path).ok()?);
    let archive = parse_archive(&std::fs::read(path).ok()?)?;
    let entry = archive.entries.iter().find(|e| e.name == entry_name)?;
    Some(entry.metadata.clone())
}

/// Sets (or clears, when `metadata` is empty) the per-file metadata for
/// `entry_name` and rewrites the archive. Fails if the entry does not exist.
fn set_file_metadata_bytes(
    archive_path: &[u8],
    entry_name: &[u8],
    metadata: &[u8],
) -> Option<()> {
    let path = std::path::Path::new(std::str::from_utf8(archive_path).ok()?);
    let mut archive = parse_archive(&std::fs::read(path).ok()?)?;
    let entry = archive.entries.iter_mut().find(|e| e.name == entry_name)?;
    entry.metadata.clear();
    entry.metadata.extend_from_slice(metadata);
    let rebuilt = build_archive_value(&archive)?;
    std::fs::write(path, rebuilt).ok()
}

/// Reads per-file metadata addressed by a `phar://archive/entry` URL, splitting it
/// into archive path and entry name before delegating to [`get_file_metadata_bytes`].
fn get_file_metadata_url(url: &[u8]) -> Option<Vec<u8>> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry) = split_archive_entry(rest)?;
    get_file_metadata_bytes(archive_path, entry)
}

/// Writes per-file metadata addressed by a `phar://archive/entry` URL, splitting it
/// into archive path and entry name before delegating to [`set_file_metadata_bytes`].
fn set_file_metadata_url(url: &[u8], metadata: &[u8]) -> Option<()> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry) = split_archive_entry(rest)?;
    set_file_metadata_bytes(archive_path, entry, metadata)
}

/// Removes an archive entry and reports failure when no matching entry exists.
fn remove_entry(entries: &mut Vec<ArchiveEntry>, entry_name: &[u8]) -> Option<()> {
    let index = entries.iter().position(|entry| entry.name == entry_name)?;
    entries.remove(index);
    Some(())
}

/// Builds a SHA1-signed native PHAR archive from decoded entries.
fn build_native_phar_archive(
    entries: &[ArchiveEntry],
    metadata: &[u8],
    stub: &[u8],
) -> Option<Vec<u8>> {
    let mut manifest = Vec::new();
    let mut stored_entries = Vec::with_capacity(entries.len());
    manifest.extend_from_slice(&u32::try_from(entries.len()).ok()?.to_le_bytes());
    manifest.extend_from_slice(&[0x11, 0x00]);
    manifest.extend_from_slice(&PHAR_HDR_SIGNATURE.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
    // Global metadata field: length-prefixed serialized blob (empty when unset).
    manifest.extend_from_slice(&u32::try_from(metadata.len()).ok()?.to_le_bytes());
    manifest.extend_from_slice(metadata);
    for entry in entries {
        let name_len = u32::try_from(entry.name.len()).ok()?;
        let payload_len = u32::try_from(entry.payload.len()).ok()?;
        let stored = encode_phar_payload(&entry.payload, entry.compression)?;
        let stored_len = u32::try_from(stored.len()).ok()?;
        manifest.extend_from_slice(&name_len.to_le_bytes());
        manifest.extend_from_slice(&entry.name);
        manifest.extend_from_slice(&payload_len.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&stored_len.to_le_bytes());
        manifest.extend_from_slice(&crc32(&entry.payload).to_le_bytes());
        manifest.extend_from_slice(
            &(PHAR_FILE_MODE_0644 | phar_compression_flag(entry.compression)).to_le_bytes(),
        );
        // Per-entry metadata field: length-prefixed serialized blob (empty when unset).
        manifest.extend_from_slice(&u32::try_from(entry.metadata.len()).ok()?.to_le_bytes());
        manifest.extend_from_slice(&entry.metadata);
        stored_entries.push(stored);
    }

    let mut out = Vec::new();
    if stub.is_empty() {
        out.extend_from_slice(PHAR_DEFAULT_STUB);
    } else {
        out.extend_from_slice(stub);
    }
    out.extend_from_slice(&u32::try_from(manifest.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&manifest);
    for stored in stored_entries {
        out.extend_from_slice(&stored);
    }
    append_sha1_signature(&mut out);
    Some(out)
}

/// Encodes a native PHAR payload according to its preserved compression mode.
fn encode_phar_payload(payload: &[u8], compression: PharCompression) -> Option<Vec<u8>> {
    match compression {
        PharCompression::None => Some(payload.to_vec()),
        PharCompression::Gzip => {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(payload).ok()?;
            encoder.finish().ok()
        }
        PharCompression::Bzip2 => {
            let mut encoder =
                bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            encoder.write_all(payload).ok()?;
            encoder.finish().ok()
        }
    }
}

/// Returns the PHAR manifest flag for a compression mode.
fn phar_compression_flag(compression: PharCompression) -> u32 {
    match compression {
        PharCompression::None => 0,
        PharCompression::Gzip => PHAR_FLAG_GZIP,
        PharCompression::Bzip2 => PHAR_FLAG_BZIP2,
    }
}

/// Converts PHP's PHAR compression constants into bridge compression modes.
fn compression_from_php_constant(value: usize) -> Option<PharCompression> {
    match value {
        0 => Some(PharCompression::None),
        4_096 => Some(PharCompression::Gzip),
        8_192 => Some(PharCompression::Bzip2),
        _ => None,
    }
}

/// Builds a POSIX ustar archive with stored regular-file entries.
fn build_tar_archive(entries: &[ArchiveEntry], metadata: &[u8], stub: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    write_tar_body(&mut out, entries, metadata, stub)?;
    out.extend_from_slice(&[0u8; 1024]);
    Some(out)
}

/// Writes a tar phar's data records (stub, global metadata, entries, and per-file
/// metadata side entries) into `out`, without the trailing zero blocks. The bytes
/// it produces are exactly the range a tar phar signature is computed over.
fn write_tar_body(
    out: &mut Vec<u8>,
    entries: &[ArchiveEntry],
    metadata: &[u8],
    stub: &[u8],
) -> Option<()> {
    // Tar-based phars store the stub and global metadata as reserved `.phar/*` files.
    if !stub.is_empty() {
        write_tar_entry(out, PHAR_STUB_ENTRY, stub)?;
    }
    if !metadata.is_empty() {
        write_tar_entry(out, PHAR_METADATA_ENTRY, metadata)?;
    }
    for entry in entries {
        write_tar_entry(out, &entry.name, &entry.payload)?;
    }
    // Per-file metadata rides in `.phar/.metadata/<path>/.metadata.bin` side entries.
    for entry in entries {
        if !entry.metadata.is_empty() {
            write_tar_entry(out, &tar_file_metadata_name(&entry.name), &entry.metadata)?;
        }
    }
    Some(())
}

/// Rebuilds a tar phar with a PHP-compatible `.phar/signature.bin` trailer entry.
///
/// The signature is computed over the data records (everything before the
/// signature entry's header), then the signature entry is appended as the last
/// record before the trailing zero blocks, matching php-src `phar_tar_flush`.
fn sign_tar_archive(archive: &Archive, flag: u32, key: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    write_tar_body(&mut out, &archive.entries, &archive.metadata, &archive.stub)?;
    let sig = compute_signature(flag, key, &out)?;
    write_tar_entry(&mut out, PHAR_SIGNATURE_ENTRY, &signature_bin_payload(flag, &sig)?)?;
    out.extend_from_slice(&[0u8; 1024]);
    Some(out)
}

/// Builds the tar side-entry path that holds one file's serialized metadata.
fn tar_file_metadata_name(entry_name: &[u8]) -> Vec<u8> {
    let mut name = Vec::with_capacity(
        PHAR_FILE_METADATA_PREFIX.len() + entry_name.len() + PHAR_FILE_METADATA_SUFFIX.len(),
    );
    name.extend_from_slice(PHAR_FILE_METADATA_PREFIX);
    name.extend_from_slice(entry_name);
    name.extend_from_slice(PHAR_FILE_METADATA_SUFFIX);
    name
}

/// Writes one uncompressed POSIX ustar entry (512-byte header + padded payload).
fn write_tar_entry(out: &mut Vec<u8>, entry_name: &[u8], payload: &[u8]) -> Option<()> {
    let (name, prefix) = split_tar_name(entry_name)?;
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name);
    if let Some(prefix) = prefix {
        header[345..345 + prefix.len()].copy_from_slice(prefix);
    }
    let mode = b"0000644\0";
    header[100..100 + mode.len()].copy_from_slice(mode);
    let uid = b"0000000\0";
    header[108..108 + uid.len()].copy_from_slice(uid);
    header[116..116 + uid.len()].copy_from_slice(uid);
    let size = format!("{:011o}\0", payload.len());
    header[124..124 + size.len()].copy_from_slice(size.as_bytes());
    let mtime = b"00000000000\0";
    header[136..136 + mtime.len()].copy_from_slice(mtime);
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    let checksum: u32 = header.iter().map(|&byte| byte as u32).sum();
    let checksum = format!("{:06o}\0 ", checksum);
    header[148..156].copy_from_slice(checksum.as_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(payload);
    out.resize(out.len() + round_up_to_512(payload.len())? - payload.len(), 0);
    Some(())
}

/// Splits a tar entry path into ustar `name` and optional `prefix` fields.
fn split_tar_name(name: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
    if name.len() <= 100 {
        return Some((name, None));
    }
    for idx in (1..name.len()).rev() {
        if name[idx] != b'/' {
            continue;
        }
        let prefix = &name[..idx];
        let leaf = &name[idx + 1..];
        if !leaf.is_empty() && prefix.len() <= 155 && leaf.len() <= 100 {
            return Some((leaf, Some(prefix)));
        }
    }
    None
}

/// Builds a ZIP archive with stored or deflated entries and central-directory records.
fn build_zip_archive(entries: &[ArchiveEntry], metadata: &[u8], stub: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let count = write_zip_body(&mut out, &mut central, entries, stub)?;
    finalize_zip(&mut out, &central, count, metadata)?;
    Some(out)
}

/// Writes a zip phar's local file entries (stub plus regular entries) into `out`
/// and their central-directory records into `central`, returning the entry count.
/// When a zip password is set, every file entry — the stub included — is
/// ZipCrypto-encrypted; only the separately-written `.phar/signature.bin` stays
/// in the clear.
fn write_zip_body(
    out: &mut Vec<u8>,
    central: &mut Vec<u8>,
    entries: &[ArchiveEntry],
    stub: &[u8],
) -> Option<usize> {
    let mut count = 0usize;
    // Zip-based phars store the stub as the reserved `.phar/stub.php` entry.
    if !stub.is_empty() {
        write_zip_entry(out, central, PHAR_STUB_ENTRY, stub, PharCompression::None, &[], true)?;
        count += 1;
    }
    for entry in entries {
        write_zip_entry(
            out,
            central,
            &entry.name,
            &entry.payload,
            entry.compression,
            &entry.metadata,
            true,
        )?;
        count += 1;
    }
    Some(count)
}

/// Appends the central directory and the end-of-central-directory record (with the
/// global metadata carried as the ZIP archive comment) to a zip phar under build.
fn finalize_zip(out: &mut Vec<u8>, central: &[u8], count: usize, metadata: &[u8]) -> Option<()> {
    let central_offset = out.len();
    let central_len = central.len();
    // Zip-based phars store global metadata in the EOCD archive comment.
    let comment_len = u16::try_from(metadata.len()).ok()?;
    out.extend_from_slice(central);

    // Emit the ZIP64 EOCD record + locator when the entry count, central-directory
    // size, or offset overflows the regular EOCD's 16-/32-bit fields.
    let sentinel = ZIP32_SENTINEL as usize;
    let needs_zip64 =
        count >= ZIP16_SENTINEL as usize || central_offset > sentinel || central_len > sentinel;
    if needs_zip64 {
        let eocd64_offset = out.len() as u64;
        // -- ZIP64 end-of-central-directory record --
        out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        out.extend_from_slice(&44u64.to_le_bytes()); // size of the rest of this record
        out.extend_from_slice(&45u16.to_le_bytes()); // version made by
        out.extend_from_slice(&45u16.to_le_bytes()); // version needed to extract
        out.extend_from_slice(&0u32.to_le_bytes()); // number of this disk
        out.extend_from_slice(&0u32.to_le_bytes()); // disk with central directory
        out.extend_from_slice(&(count as u64).to_le_bytes()); // entries on this disk
        out.extend_from_slice(&(count as u64).to_le_bytes()); // total entries
        out.extend_from_slice(&(central_len as u64).to_le_bytes());
        out.extend_from_slice(&(central_offset as u64).to_le_bytes());
        // -- ZIP64 end-of-central-directory locator --
        out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // disk with the ZIP64 EOCD
        out.extend_from_slice(&eocd64_offset.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes()); // total number of disks
    }

    // Regular EOCD, using the 0xFFFF / 0xFFFFFFFF sentinels for overflowed fields.
    let entry_count = u16::try_from(count).unwrap_or(ZIP16_SENTINEL);
    let cd_len = u32::try_from(central_len).unwrap_or(ZIP32_SENTINEL);
    let cd_offset = u32::try_from(central_offset).unwrap_or(ZIP32_SENTINEL);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&entry_count.to_le_bytes());
    out.extend_from_slice(&entry_count.to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&comment_len.to_le_bytes());
    out.extend_from_slice(metadata);
    Some(())
}

/// Rebuilds a zip phar with a PHP-compatible `.phar/signature.bin` entry.
///
/// php-src `phar_zip_applysignature` hashes the local file entries, the central
/// directory, and the archive comment — but not the EOCD — and then appends the
/// signature as the archive's last local entry and last central record.
fn sign_zip_archive(archive: &Archive, flag: u32, key: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut count = write_zip_body(&mut out, &mut central, &archive.entries, &archive.stub)?;
    // Signed range: local entries ++ central records ++ comment, signature excluded.
    let mut signed = out.clone();
    signed.extend_from_slice(&central);
    signed.extend_from_slice(&archive.metadata);
    let sig = compute_signature(flag, key, &signed)?;
    // The signature entry stays in the clear — a verifier must read it without the
    // password (and the signature already covers the encrypted local-entry bytes).
    write_zip_entry(
        &mut out,
        &mut central,
        PHAR_SIGNATURE_ENTRY,
        &signature_bin_payload(flag, &sig)?,
        PharCompression::None,
        &[],
        false,
    )?;
    count += 1;
    finalize_zip(&mut out, &central, count, &archive.metadata)?;
    Some(out)
}

/// Writes one ZIP entry: its local file header + stored payload into `out`, and the
/// matching central-directory record into `central`. When `encrypt` is set and a zip
/// password is configured, the stored payload is ZipCrypto-encrypted and the
/// general-purpose "encrypted" flag is set in both headers.
fn write_zip_entry(
    out: &mut Vec<u8>,
    central: &mut Vec<u8>,
    name: &[u8],
    payload: &[u8],
    compression: PharCompression,
    metadata: &[u8],
    encrypt: bool,
) -> Option<()> {
    let name_len = u16::try_from(name.len()).ok()?;
    let comment_len = u16::try_from(metadata.len()).ok()?;
    let payload_len = payload.len();
    let (method, stored) = encode_zip_payload(payload, compression)?;
    let local_offset = out.len();
    let crc = crc32(payload);

    // Encrypt the stored payload (traditional ZipCrypto) when requested and a zip
    // password is set. The 12-byte header's check byte is the CRC's high byte, since
    // no data descriptor is written — matching the read-side `zip_entry_crypto`
    // branch. Encryption grows the stored size by 12 bytes and sets flag bit 0.
    let password = if encrypt { current_zip_password() } else { None };
    let (stored, flags) = match password {
        Some(pw) => (
            zipcrypto_encrypt(&pw, &stored, (crc >> 24) as u8),
            ZIP_FLAG_ENCRYPTED,
        ),
        None => (stored, 0u16),
    };
    let stored_len = stored.len();

    // ZIP64 is needed when a size or the local-header offset overflows 32 bits.
    let sentinel = ZIP32_SENTINEL as usize;
    let zip64_sizes = stored_len > sentinel || payload_len > sentinel;
    let zip64_offset = local_offset > sentinel;
    let version: u16 = if zip64_sizes || zip64_offset { 45 } else { 20 };

    // Local header: defers both sizes to a ZIP64 extra field once either overflows.
    let local_csz = if zip64_sizes { ZIP32_SENTINEL } else { stored_len as u32 };
    let local_usz = if zip64_sizes { ZIP32_SENTINEL } else { payload_len as u32 };
    let local_extra = if zip64_sizes {
        zip64_local_extra(payload_len as u64, stored_len as u64)
    } else {
        Vec::new()
    };
    let local_extra_len = u16::try_from(local_extra.len()).ok()?;

    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&local_csz.to_le_bytes());
    out.extend_from_slice(&local_usz.to_le_bytes());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&local_extra_len.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&local_extra);
    out.extend_from_slice(&stored);

    // Central record: each overflowed field becomes a sentinel + a ZIP64 extra entry.
    let cen_csz = if stored_len > sentinel { ZIP32_SENTINEL } else { stored_len as u32 };
    let cen_usz = if payload_len > sentinel { ZIP32_SENTINEL } else { payload_len as u32 };
    let cen_off = if zip64_offset { ZIP32_SENTINEL } else { local_offset as u32 };
    let cen_extra = if zip64_sizes || zip64_offset {
        zip64_central_extra(
            (payload_len > sentinel).then_some(payload_len as u64),
            (stored_len > sentinel).then_some(stored_len as u64),
            zip64_offset.then_some(local_offset as u64),
        )
    } else {
        Vec::new()
    };
    let cen_extra_len = u16::try_from(cen_extra.len()).ok()?;

    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&version.to_le_bytes());
    central.extend_from_slice(&version.to_le_bytes());
    central.extend_from_slice(&flags.to_le_bytes());
    central.extend_from_slice(&method.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&crc.to_le_bytes());
    central.extend_from_slice(&cen_csz.to_le_bytes());
    central.extend_from_slice(&cen_usz.to_le_bytes());
    central.extend_from_slice(&name_len.to_le_bytes());
    central.extend_from_slice(&cen_extra_len.to_le_bytes());
    // File comment length: carries this entry's serialized per-file metadata.
    central.extend_from_slice(&comment_len.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes());
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(&cen_off.to_le_bytes());
    central.extend_from_slice(name);
    central.extend_from_slice(&cen_extra);
    central.extend_from_slice(metadata);
    Some(())
}

/// Builds a ZIP64 local-header extra field carrying the 64-bit uncompressed and
/// compressed sizes (tag 0x0001, both fields always present in local headers).
fn zip64_local_extra(uncompressed: u64, compressed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&ZIP64_EXTRA_TAG.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&uncompressed.to_le_bytes());
    out.extend_from_slice(&compressed.to_le_bytes());
    out
}

/// Builds a ZIP64 central-directory extra field holding only the overflowed
/// fields, in APPNOTE order: uncompressed size, compressed size, header offset.
fn zip64_central_extra(
    uncompressed: Option<u64>,
    compressed: Option<u64>,
    offset: Option<u64>,
) -> Vec<u8> {
    let mut body = Vec::new();
    for field in [uncompressed, compressed, offset].into_iter().flatten() {
        body.extend_from_slice(&field.to_le_bytes());
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&ZIP64_EXTRA_TAG.to_le_bytes());
    out.extend_from_slice(&(body.len() as u16).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Encodes a ZIP entry payload and returns its ZIP compression method.
fn encode_zip_payload(payload: &[u8], compression: PharCompression) -> Option<(u16, Vec<u8>)> {
    match compression {
        PharCompression::None => Some((ZIP_METHOD_STORE, payload.to_vec())),
        PharCompression::Gzip => {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(payload).ok()?;
            Some((ZIP_METHOD_DEFLATE, encoder.finish().ok()?))
        }
        PharCompression::Bzip2 => None,
    }
}

/// Appends PHP's raw-SHA1 PHAR signature trailer to `archive`.
fn append_sha1_signature(archive: &mut Vec<u8>) {
    use sha1::{Digest, Sha1};

    let digest = Sha1::digest(&archive);
    archive.extend_from_slice(&digest);
    archive.extend_from_slice(&PHAR_SHA1_SIGNATURE_TYPE.to_le_bytes());
    archive.extend_from_slice(b"GBMB");
}

/// Returns the raw digest length for a PHP hash-based PHAR signature flag
/// (MD5=1, SHA1=2, SHA256=3, SHA512=4); `None` for non-hash flags.
fn signature_digest_len(flags: u32) -> Option<usize> {
    match flags {
        1 => Some(16),
        2 => Some(20),
        3 => Some(32),
        4 => Some(64),
        _ => None,
    }
}

/// Returns the archive bytes with any trailing PHP signature trailer removed
/// (native PHAR `digest ++ LE32(flag) ++ "GBMB"`, or the OpenSSL variant
/// `sig ++ LE32(sig_len) ++ LE32(0x10) ++ "GBMB"`). Returns the input unchanged
/// when no recognized trailer is present.
fn strip_signature_trailer(archive: &[u8]) -> &[u8] {
    let n = archive.len();
    if n < 8 || &archive[n - 4..] != b"GBMB" {
        return archive;
    }
    let flags = u32::from_le_bytes(archive[n - 8..n - 4].try_into().unwrap());
    if flags == PHAR_OPENSSL_SIGNATURE_TYPE {
        if n >= 12 {
            let sig_len = u32::from_le_bytes(archive[n - 12..n - 8].try_into().unwrap()) as usize;
            if let Some(total) = sig_len.checked_add(12) {
                if n >= total {
                    return &archive[..n - total];
                }
            }
        }
    } else if let Some(dlen) = signature_digest_len(flags) {
        let total = dlen + 8;
        if n >= total {
            return &archive[..n - total];
        }
    }
    archive
}

/// Computes the PKCS#1 v1.5 RSA-SHA1 signature of `data` with a PEM private key
/// (PKCS#8 or PKCS#1), matching PHP's `openssl_sign(..., OPENSSL_ALGO_SHA1)`.
fn rsa_sha1_sign(data: &[u8], key_pem: &[u8]) -> Option<Vec<u8>> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::{Pkcs1v15Sign, RsaPrivateKey};
    use sha1::{Digest, Sha1};

    let pem = std::str::from_utf8(key_pem).ok()?;
    let key = RsaPrivateKey::from_pkcs8_pem(pem)
        .ok()
        .or_else(|| RsaPrivateKey::from_pkcs1_pem(pem).ok())?;
    let hashed = Sha1::digest(data);
    key.sign(Pkcs1v15Sign::new::<Sha1>(), &hashed).ok()
}

/// Computes a PHP-compatible signature over `data` for a signature `flag`: a raw
/// MD5/SHA1/SHA256/SHA512 digest (flags 1..=4) or an RSA-SHA1 OpenSSL signature
/// (flag 0x10, requiring the PEM `key`). Returns `None` for an unknown flag or a
/// missing/invalid key.
fn compute_signature(flag: u32, key: Option<&[u8]>, data: &[u8]) -> Option<Vec<u8>> {
    use md5::Md5;
    use sha1::{Digest, Sha1};
    use sha2::{Sha256, Sha512};

    match flag {
        1 => Some(Md5::digest(data).to_vec()),
        2 => Some(Sha1::digest(data).to_vec()),
        3 => Some(Sha256::digest(data).to_vec()),
        4 => Some(Sha512::digest(data).to_vec()),
        PHAR_OPENSSL_SIGNATURE_TYPE => rsa_sha1_sign(data, key?),
        _ => None,
    }
}

/// Builds the `.phar/signature.bin` payload for a tar/zip phar:
/// `LE32(sig_flag) ++ LE32(sig_len) ++ signature`.
fn signature_bin_payload(flag: u32, sig: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(8 + sig.len());
    out.extend_from_slice(&flag.to_le_bytes());
    out.extend_from_slice(&u32::try_from(sig.len()).ok()?.to_le_bytes());
    out.extend_from_slice(sig);
    Some(out)
}

/// Detects the archive family of `data` for signature operations: zip (PK magic),
/// tar (ustar magic at offset 257), or native PHAR (default). Returns `None` for a
/// gzip/bzip2-wrapped archive, where signature rewriting is not supported.
fn signing_format(data: &[u8]) -> Option<ArchiveFormat> {
    if data.starts_with(&[0x50, 0x4b, 0x03, 0x04]) || data.starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
        Some(ArchiveFormat::Zip)
    } else if data.get(257..262) == Some(b"ustar") {
        Some(ArchiveFormat::Tar)
    } else if data.starts_with(&[0x1f, 0x8b]) || data.starts_with(b"BZh") {
        None
    } else {
        Some(ArchiveFormat::NativePhar)
    }
}

/// Re-signs the phar at `path` with an OpenSSL (RSA-SHA1) signature. Native PHARs
/// gain a `sig ++ LE32(sig_len) ++ LE32(0x10) ++ "GBMB"` trailer; tar/zip phars
/// gain a `.phar/signature.bin` entry. The caller-supplied public key is what
/// verifiers use; PHP does not auto-write a `.pubkey` here either.
fn sign_archive_openssl(path: &[u8], key_pem: &[u8]) -> Option<()> {
    let data = read_path(path)?;
    match signing_format(&data)? {
        ArchiveFormat::Zip => {
            let signed =
                sign_zip_archive(&parse_zip_archive(&data)?, PHAR_OPENSSL_SIGNATURE_TYPE, Some(key_pem))?;
            write_path(path, &signed)
        }
        ArchiveFormat::Tar => {
            let signed =
                sign_tar_archive(&parse_tar_archive(&data)?, PHAR_OPENSSL_SIGNATURE_TYPE, Some(key_pem))?;
            write_path(path, &signed)
        }
        ArchiveFormat::NativePhar => {
            let mut out = strip_signature_trailer(&data).to_vec();
            let sig = rsa_sha1_sign(&out, key_pem)?;
            out.extend_from_slice(&sig);
            out.extend_from_slice(&u32::try_from(sig.len()).ok()?.to_le_bytes());
            out.extend_from_slice(&PHAR_OPENSSL_SIGNATURE_TYPE.to_le_bytes());
            out.extend_from_slice(b"GBMB");
            write_path(path, &out)
        }
    }
}

/// Re-signs the phar at `path` with a hash-based signature (MD5/SHA1/SHA256/SHA512
/// per `algo` 1..=4). Native PHARs append `digest ++ LE32(algo) ++ "GBMB"`; tar/zip
/// phars gain a `.phar/signature.bin` entry.
fn sign_archive_hash(path: &[u8], algo: u32) -> Option<()> {
    let data = read_path(path)?;
    match signing_format(&data)? {
        ArchiveFormat::Zip => {
            let signed = sign_zip_archive(&parse_zip_archive(&data)?, algo, None)?;
            write_path(path, &signed)
        }
        ArchiveFormat::Tar => {
            let signed = sign_tar_archive(&parse_tar_archive(&data)?, algo, None)?;
            write_path(path, &signed)
        }
        ArchiveFormat::NativePhar => {
            let mut out = strip_signature_trailer(&data).to_vec();
            let digest = compute_signature(algo, None, &out)?;
            out.extend_from_slice(&digest);
            out.extend_from_slice(&algo.to_le_bytes());
            out.extend_from_slice(b"GBMB");
            write_path(path, &out)
        }
    }
}

/// Decodes a tar/zip `.phar/signature.bin` payload into its flag and signature
/// bytes (`LE32(flag) ++ LE32(len) ++ signature`).
fn parse_signature_bin(payload: &[u8]) -> Option<(u32, Vec<u8>)> {
    let flag = le32(payload, 0)?;
    let len = le32(payload, 4)? as usize;
    Some((flag, payload.get(8..8usize.checked_add(len)?)?.to_vec()))
}

/// Returns the raw `.phar/signature.bin` payload from a tar phar, if present.
fn read_tar_signature(data: &[u8]) -> Option<Vec<u8>> {
    let mut p = 0usize;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let size = parse_tar_octal(&header[124..136])?;
        let payload_start = p.checked_add(512)?;
        let typeflag = header[156];
        if (typeflag == 0 || typeflag == b'0') && tar_entry_name(header)? == PHAR_SIGNATURE_ENTRY {
            return data
                .get(payload_start..payload_start.checked_add(size)?)
                .map(<[u8]>::to_vec);
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    None
}

/// Returns the raw `.phar/signature.bin` payload from a zip phar, if present.
fn read_zip_signature(data: &[u8]) -> Option<Vec<u8>> {
    let (entry_count, central_dir_offset) = zip_eocd_info(data)?;
    let mut p = central_dir_offset;
    for _ in 0..entry_count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        let method = le16(data, p + 10)?;
        let mut compressed_size = le32(data, p + 20)? as usize;
        let mut uncompressed_size = le32(data, p + 24)? as usize;
        let name_len = le16(data, p + 28)? as usize;
        let extra_len = le16(data, p + 30)? as usize;
        let comment_len = le16(data, p + 32)? as usize;
        let mut local_offset = le32(data, p + 42)? as usize;
        let name_start = p + 46;
        let name = data.get(name_start..name_start.checked_add(name_len)?)?;
        if name == PHAR_SIGNATURE_ENTRY {
            apply_zip64_central_extra(
                data,
                name_start.checked_add(name_len)?,
                extra_len,
                &mut uncompressed_size,
                &mut compressed_size,
                &mut local_offset,
            )?;
            // The reserved signature entry is never encrypted.
            return decode_zip_local_entry(
                data,
                local_offset,
                method,
                compressed_size,
                uncompressed_size,
                false,
                0,
            );
        }
        p = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(comment_len)?;
    }
    None
}

/// Reads the signature of the phar at `path`, returning the flag and the raw
/// signature/digest bytes. Native PHARs use the `GBMB` trailer; tar/zip phars use
/// the `.phar/signature.bin` entry.
fn read_signature_info(path: &[u8]) -> Option<(u32, Vec<u8>)> {
    let data = read_path(path)?;
    match signing_format(&data)? {
        ArchiveFormat::Zip => parse_signature_bin(&read_zip_signature(&data)?),
        ArchiveFormat::Tar => parse_signature_bin(&read_tar_signature(&data)?),
        ArchiveFormat::NativePhar => {
            let n = data.len();
            if n < 8 || &data[n - 4..] != b"GBMB" {
                return None;
            }
            let flags = u32::from_le_bytes(data[n - 8..n - 4].try_into().unwrap());
            if flags == PHAR_OPENSSL_SIGNATURE_TYPE {
                let sig_len =
                    u32::from_le_bytes(data.get(n - 12..n - 8)?.try_into().unwrap()) as usize;
                let start = n.checked_sub(12)?.checked_sub(sig_len)?;
                Some((flags, data.get(start..n - 12)?.to_vec()))
            } else {
                let dlen = signature_digest_len(flags)?;
                let start = n.checked_sub(8)?.checked_sub(dlen)?;
                Some((flags, data.get(start..n - 8)?.to_vec()))
            }
        }
    }
}

/// Returns the uppercase hex of the PHAR's signature/digest bytes (PHP
/// `Phar::getSignature()['hash']`).
fn signature_hash_hex(path: &[u8]) -> Option<Vec<u8>> {
    let (_, bytes) = read_signature_info(path)?;
    let mut hex = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.extend_from_slice(format!("{byte:02X}").as_bytes());
    }
    Some(hex)
}

/// Returns the PHP signature type name for the PHAR (`getSignature()['hash_type']`).
fn signature_type_name(path: &[u8]) -> Option<Vec<u8>> {
    let (flags, _) = read_signature_info(path)?;
    let name: &[u8] = match flags {
        1 => b"MD5",
        2 => b"SHA-1",
        3 => b"SHA-256",
        4 => b"SHA-512",
        PHAR_OPENSSL_SIGNATURE_TYPE => b"OpenSSL",
        _ => return None,
    };
    Some(name.to_vec())
}

/// Computes PHP-compatible reflected CRC32 for a PHAR entry payload.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Parses a ZIP archive central directory and returns a store/deflate entry.
fn parse_zip_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    parse_zip_archive(data)?
        .entries
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a zip-based phar into entries plus its global metadata and stub.
///
/// Global metadata is read from the EOCD archive comment; the reserved
/// `.phar/stub.php` entry becomes the stub and other `.phar/*` control entries are
/// hidden from the entry listing.
fn parse_zip_archive(data: &[u8]) -> Option<Archive> {
    let eocd = find_zip_eocd(data)?;
    let (entry_count, central_dir_offset) = zip_eocd_info(data)?;
    let comment_len = le16(data, eocd + 20)? as usize;
    let comment_start = eocd.checked_add(22)?;
    let metadata = data
        .get(comment_start..comment_start.checked_add(comment_len)?)?
        .to_vec();
    let mut entries = Vec::with_capacity(entry_count.min(1 << 16));
    let mut stub = Vec::new();
    let mut p = central_dir_offset;
    for _ in 0..entry_count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        // A data-descriptor entry (general-purpose flag bit 3) carries zeroed
        // CRC/sizes in its local header and the real values in the central
        // directory we are already reading here, so it needs no special handling
        // beyond trusting these central-directory sizes.
        let method = le16(data, p + 10)?;
        let mut compressed_size = le32(data, p + 20)? as usize;
        let mut uncompressed_size = le32(data, p + 24)? as usize;
        let name_len = le16(data, p + 28)? as usize;
        let extra_len = le16(data, p + 30)? as usize;
        let entry_comment_len = le16(data, p + 32)? as usize;
        let mut local_offset = le32(data, p + 42)? as usize;
        let name_start = p + 46;
        let name = data.get(name_start..name_start.checked_add(name_len)?)?;
        // ZIP64: sentinel size/offset fields defer to the central record's extra.
        apply_zip64_central_extra(
            data,
            name_start.checked_add(name_len)?,
            extra_len,
            &mut uncompressed_size,
            &mut compressed_size,
            &mut local_offset,
        )?;
        let (encrypted, check_byte) = zip_entry_crypto(data, p)?;
        let payload = decode_zip_local_entry(
            data,
            local_offset,
            method,
            compressed_size,
            uncompressed_size,
            encrypted,
            check_byte,
        )?;
        let comment_start = name_start.checked_add(name_len)?.checked_add(extra_len)?;
        if name == PHAR_STUB_ENTRY {
            stub = payload;
        } else if !is_phar_control_entry(name) {
            let compression = zip_compression_from_method(method)?;
            // Per-file metadata rides in the central-directory file comment.
            let entry_metadata = data
                .get(comment_start..comment_start.checked_add(entry_comment_len)?)?
                .to_vec();
            entries.push(ArchiveEntry {
                name: name.to_vec(),
                payload,
                compression,
                metadata: entry_metadata,
            });
        }
        p = comment_start.checked_add(entry_comment_len)?;
    }
    Some(Archive {
        entries,
        format: ArchiveFormat::Zip,
        metadata,
        stub,
    })
}

/// Maps supported ZIP methods to the bridge's compression representation.
fn zip_compression_from_method(method: u16) -> Option<PharCompression> {
    match method {
        ZIP_METHOD_STORE => Some(PharCompression::None),
        ZIP_METHOD_DEFLATE => Some(PharCompression::Gzip),
        _ => None,
    }
}

/// Finds the ZIP end-of-central-directory record.
fn find_zip_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let start = data.len().saturating_sub(65_557);
    (start..=data.len() - 22)
        .rev()
        .find(|&i| data.get(i..i + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
}

/// Returns a ZIP archive's `(total entry count, central-directory offset)`,
/// transparently following the ZIP64 EOCD record when the regular EOCD uses
/// sentinels for an entry count, central-directory size, or offset that overflows
/// its 32-/16-bit field.
fn zip_eocd_info(data: &[u8]) -> Option<(usize, usize)> {
    let eocd = find_zip_eocd(data)?;
    let mut entry_count = le16(data, eocd + 10)? as usize;
    let cd_size = le32(data, eocd + 12)?;
    let mut cd_offset = le32(data, eocd + 16)? as usize;
    let needs_zip64 = le16(data, eocd + 10)? == ZIP16_SENTINEL
        || cd_size == ZIP32_SENTINEL
        || cd_offset as u32 == ZIP32_SENTINEL;
    if needs_zip64 {
        if let Some((count, offset)) = read_zip64_eocd(data, eocd) {
            entry_count = count;
            cd_offset = offset;
        }
    }
    Some((entry_count, cd_offset))
}

/// Reads the ZIP64 end-of-central-directory record (located via the 20-byte
/// locator immediately before the regular EOCD), returning its 64-bit total entry
/// count and central-directory offset.
fn read_zip64_eocd(data: &[u8], eocd: usize) -> Option<(usize, usize)> {
    let locator = eocd.checked_sub(20)?;
    if le32(data, locator)? != 0x0706_4b50 {
        return None;
    }
    let eocd64 = le64(data, locator + 8)? as usize;
    if le32(data, eocd64)? != 0x0606_4b50 {
        return None;
    }
    let total_entries = le64(data, eocd64 + 32)? as usize;
    let cd_offset = le64(data, eocd64 + 48)? as usize;
    Some((total_entries, cd_offset))
}

/// Overrides any sentinel (`0xFFFFFFFF`) compressed size, uncompressed size, or
/// local-header offset of a ZIP central record with the 64-bit value from its
/// ZIP64 extra field (tag 0x0001). The extra field lists only the overflowed
/// fields, in the fixed order: original size, compressed size, header offset.
fn apply_zip64_central_extra(
    data: &[u8],
    extra_start: usize,
    extra_len: usize,
    uncompressed: &mut usize,
    compressed: &mut usize,
    local_offset: &mut usize,
) -> Option<()> {
    let end = extra_start.checked_add(extra_len)?;
    let mut p = extra_start;
    while p.checked_add(4)? <= end {
        let tag = le16(data, p)?;
        let size = le16(data, p + 2)? as usize;
        let body = p + 4;
        if tag == ZIP64_EXTRA_TAG {
            let mut q = body;
            if *uncompressed as u32 == ZIP32_SENTINEL {
                *uncompressed = le64(data, q)? as usize;
                q += 8;
            }
            if *compressed as u32 == ZIP32_SENTINEL {
                *compressed = le64(data, q)? as usize;
                q += 8;
            }
            if *local_offset as u32 == ZIP32_SENTINEL {
                *local_offset = le64(data, q)? as usize;
            }
            return Some(());
        }
        p = body.checked_add(size)?;
    }
    Some(())
}

/// Reads a ZIP central record's encryption state: whether the entry is ZipCrypto
/// encrypted (flag bit 0) and the password check byte (the high byte of the mod
/// time for data-descriptor entries, otherwise of the CRC).
fn zip_entry_crypto(data: &[u8], central_off: usize) -> Option<(bool, u8)> {
    let flags = le16(data, central_off + 8)?;
    let encrypted = flags & ZIP_FLAG_ENCRYPTED != 0;
    let check_byte = if flags & ZIP_FLAG_DATA_DESCRIPTOR != 0 {
        (le16(data, central_off + 12)? >> 8) as u8
    } else {
        (le32(data, central_off + 16)? >> 24) as u8
    };
    Some((encrypted, check_byte))
}

/// Decodes a ZIP local file payload using sizes from its central directory.
///
/// `encrypted` marks a traditional-PKWARE (ZipCrypto) entry; `check_byte` is the
/// expected last byte of its 12-byte encryption header used to reject a wrong
/// password. Encrypted entries require a password set via
/// [`elephc_phar_set_zip_password`]; without one (or with the wrong one) they
/// return `None`.
fn decode_zip_local_entry(
    data: &[u8],
    local_offset: usize,
    method: u16,
    compressed_size: usize,
    uncompressed_size: usize,
    encrypted: bool,
    check_byte: u8,
) -> Option<Vec<u8>> {
    if le32(data, local_offset)? != 0x0403_4b50 {
        return None;
    }
    let local_name_len = le16(data, local_offset + 26)? as usize;
    let local_extra_len = le16(data, local_offset + 28)? as usize;
    let payload_start = local_offset
        .checked_add(30)?
        .checked_add(local_name_len)?
        .checked_add(local_extra_len)?;
    let stored = data.get(payload_start..payload_start.checked_add(compressed_size)?)?;
    // Traditional ZipCrypto entries carry a 12-byte encryption header that the
    // password-derived keystream removes before the (optionally deflated) payload.
    let decrypted;
    let body: &[u8] = if encrypted {
        let password = current_zip_password()?;
        decrypted = zipcrypto_decrypt(&password, stored, check_byte)?;
        &decrypted
    } else {
        stored
    };
    match method {
        ZIP_METHOD_STORE => Some(body.to_vec()),
        ZIP_METHOD_DEFLATE => {
            let mut out = Vec::with_capacity(uncompressed_size);
            let mut decoder = flate2::read::DeflateDecoder::new(body);
            decoder.read_to_end(&mut out).ok()?;
            (out.len() == uncompressed_size).then_some(out)
        }
        _ => None,
    }
}

/// Traditional-PKWARE (ZipCrypto) cipher state: three 32-bit keys advanced per
/// plaintext byte. Drives both reading and writing of encrypted entries.
/// Cryptographically weak — kept only for compatibility with legacy ZipCrypto
/// archives, not as a real confidentiality mechanism.
struct ZipCryptoKeys {
    k0: u32,
    k1: u32,
    k2: u32,
}

impl ZipCryptoKeys {
    /// Seeds the keys from the password (PKWARE's fixed initial constants).
    fn new(password: &[u8]) -> Self {
        let mut keys = Self {
            k0: 0x1234_5678,
            k1: 0x2345_6789,
            k2: 0x3456_7890,
        };
        for &byte in password {
            keys.update(byte);
        }
        keys
    }

    /// Advances the three keys with one plaintext byte.
    fn update(&mut self, byte: u8) {
        self.k0 = crc32_byte(self.k0, byte);
        self.k1 = self.k1.wrapping_add(self.k0 & 0xff);
        self.k1 = self.k1.wrapping_mul(134_775_813).wrapping_add(1);
        self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8);
    }

    /// Returns the next keystream byte (derived from `k2`).
    fn keystream(&self) -> u8 {
        let temp = (self.k2 | 2) & 0xffff;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    /// Decrypts one ciphertext byte and advances the keys with the plaintext.
    fn decrypt(&mut self, cipher: u8) -> u8 {
        let plain = cipher ^ self.keystream();
        self.update(plain);
        plain
    }

    /// Encrypts one plaintext byte and advances the keys with that plaintext.
    fn encrypt(&mut self, plain: u8) -> u8 {
        let cipher = plain ^ self.keystream();
        self.update(plain);
        cipher
    }
}

/// One-byte CRC32 step (poly 0xEDB88320) used by the ZipCrypto key schedule.
fn crc32_byte(crc: u32, byte: u8) -> u32 {
    let mut t = (crc ^ byte as u32) & 0xff;
    for _ in 0..8 {
        t = if t & 1 != 0 { (t >> 1) ^ 0xedb8_8320 } else { t >> 1 };
    }
    (crc >> 8) ^ t
}

/// Decrypts a ZipCrypto entry payload (12-byte header + ciphertext) with
/// `password`, returning the post-header plaintext. Returns `None` when the data
/// is too short or the header's check byte rejects the password.
fn zipcrypto_decrypt(password: &[u8], data: &[u8], check_byte: u8) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let mut keys = ZipCryptoKeys::new(password);
    let mut header_last = 0u8;
    for &byte in &data[..12] {
        header_last = keys.decrypt(byte);
    }
    if header_last != check_byte {
        return None;
    }
    Some(data[12..].iter().map(|&c| keys.decrypt(c)).collect())
}

/// Encrypts `data` as a traditional-PKWARE (ZipCrypto) entry payload with
/// `password`: prepends a 12-byte encryption header (11 pseudo-random filler bytes
/// plus `check_byte` at index 11) and returns the encrypted `header ++ data`, which
/// is 12 bytes longer than `data`. `check_byte` must be the byte the reader will
/// verify (the CRC's high byte when no data descriptor is used). The first 11 header
/// bytes are never read back, so their randomness affects only resistance to attack,
/// not round-trip correctness.
fn zipcrypto_encrypt(password: &[u8], data: &[u8], check_byte: u8) -> Vec<u8> {
    let mut header = [0u8; 12];
    header[..11].copy_from_slice(&zipcrypto_header_filler());
    header[11] = check_byte;
    let mut keys = ZipCryptoKeys::new(password);
    let mut out = Vec::with_capacity(data.len() + 12);
    for &plain in header.iter().chain(data) {
        out.push(keys.encrypt(plain));
    }
    out
}

/// Produces 11 non-constant filler bytes for a ZipCrypto encryption header, mixing
/// a per-call atomic nonce with the current time through an xorshift64* step.
/// Dependency-free; only needs to avoid an all-constant header, since the bytes are
/// discarded on read.
fn zipcrypto_header_filler() -> [u8; 11] {
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = now ^ NONCE.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut filler = [0u8; 11];
    for byte in filler.iter_mut() {
        // xorshift64* advance, then take a high byte of the scrambled state.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        *byte = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as u8;
    }
    filler
}

/// Returns the password currently set for reading and writing encrypted ZIP
/// entries, if any.
fn current_zip_password() -> Option<Vec<u8>> {
    ZIP_PASSWORD.with(|slot| slot.borrow().clone())
}

/// Sets (or, when empty, clears) the password used to read and write encrypted
/// ZIP entries.
fn set_zip_password(password: &[u8]) {
    ZIP_PASSWORD.with(|slot| {
        *slot.borrow_mut() = if password.is_empty() {
            None
        } else {
            Some(password.to_vec())
        };
    });
}

/// Parses a POSIX tar archive and returns a regular-file entry.
fn parse_tar_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    parse_tar_archive(data)?
        .entries
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a tar-based phar into regular entries plus its global metadata and stub.
///
/// The reserved `.phar/stub.php` and `.phar/.metadata.bin` files become the stub and
/// metadata; any other `.phar/*` control file is hidden from the entry listing.
fn parse_tar_archive(data: &[u8]) -> Option<Archive> {
    let mut p = 0usize;
    let mut entries = Vec::new();
    let mut metadata = Vec::new();
    let mut stub = Vec::new();
    // Per-file metadata side entries may appear after their target entry; collect
    // them and attach once the full entry list is known.
    let mut file_metadata: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut first_header = true;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&b| b == 0) {
            break;
        }
        // Require the POSIX ustar magic on the first record so non-tar inputs
        // (e.g. native PHARs whose stub contains `__HALT_COMPILER();`) are rejected
        // rather than mis-parsed as tar.
        if first_header && header.get(257..262) != Some(b"ustar") {
            return None;
        }
        first_header = false;
        let size = parse_tar_octal(&header[124..136])?;
        let payload_start = p.checked_add(512)?;
        let payload_end = payload_start.checked_add(size)?;
        let payload = data.get(payload_start..payload_end)?;
        let typeflag = header[156];
        if typeflag == 0 || typeflag == b'0' {
            let name = tar_entry_name(header)?;
            if name == PHAR_STUB_ENTRY {
                stub = payload.to_vec();
            } else if name == PHAR_METADATA_ENTRY {
                metadata = payload.to_vec();
            } else if let Some(target) = tar_file_metadata_target(&name) {
                file_metadata.push((target, payload.to_vec()));
            } else if !is_phar_control_entry(&name) {
                entries.push(ArchiveEntry {
                    name,
                    payload: payload.to_vec(),
                    compression: PharCompression::None,
                    metadata: Vec::new(),
                });
            }
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    for (target, meta) in file_metadata {
        if let Some(entry) = entries.iter_mut().find(|e| e.name == target) {
            entry.metadata = meta;
        }
    }
    Some(Archive {
        entries,
        format: ArchiveFormat::Tar,
        metadata,
        stub,
    })
}

/// If `name` is a `.phar/.metadata/<path>/.metadata.bin` side entry, returns the
/// target entry path `<path>`; otherwise returns `None`.
fn tar_file_metadata_target(name: &[u8]) -> Option<Vec<u8>> {
    let rest = name.strip_prefix(PHAR_FILE_METADATA_PREFIX)?;
    let inner = rest.strip_suffix(PHAR_FILE_METADATA_SUFFIX)?;
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_vec())
}

/// Builds the full tar path from the `prefix` and `name` header fields.
fn tar_entry_name(header: &[u8]) -> Option<Vec<u8>> {
    let name = trim_nul_and_space(header.get(0..100)?);
    let prefix = trim_nul_and_space(header.get(345..500)?);
    if prefix.is_empty() {
        Some(name.to_vec())
    } else {
        let mut out = Vec::with_capacity(prefix.len() + 1 + name.len());
        out.extend_from_slice(prefix);
        out.push(b'/');
        out.extend_from_slice(name);
        Some(out)
    }
}

/// Parses a tar octal integer field.
fn parse_tar_octal(field: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut saw_digit = false;
    for &byte in field {
        if byte == 0 || byte == b' ' {
            if saw_digit {
                break;
            }
            continue;
        }
        if !(b'0'..=b'7').contains(&byte) {
            return None;
        }
        saw_digit = true;
        value = value.checked_mul(8)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

/// Rounds a tar payload length up to the next 512-byte block count.
fn round_up_to_512(len: usize) -> Option<usize> {
    len.checked_add(511).map(|n| (n / 512) * 512)
}

/// Trims a NUL-terminated, space-padded archive field.
fn trim_nul_and_space(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut end = end;
    while end > 0 && bytes[end - 1] == b' ' {
        end -= 1;
    }
    &bytes[..end]
}

/// Reads a little-endian `u16` from `data`.
fn le16(data: &[u8], off: usize) -> Option<u16> {
    let b = data.get(off..off + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

/// Reads a little-endian `u32` from `data`.
fn le32(data: &[u8], off: usize) -> Option<u32> {
    let b = data.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Reads a little-endian `u64` from `data` (used for ZIP64 fields).
fn le64(data: &[u8], off: usize) -> Option<u64> {
    let b = data.get(off..off + 8)?;
    Some(u64::from_le_bytes(b.try_into().ok()?))
}

/// Returns the offset of `needle` in `hay`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// Builds a minimal native PHAR fixture with entries carrying explicit flags.
    fn build_native_phar_with_flags(entries: &[(&str, &[u8], u32, u32)]) -> Vec<u8> {
        let mut manifest = Vec::new();
        manifest.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        manifest.extend_from_slice(&[0x11, 0x00]);
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        for (name, stored, uncompressed_len, flags) in entries {
            manifest.extend_from_slice(&(name.len() as u32).to_le_bytes());
            manifest.extend_from_slice(name.as_bytes());
            manifest.extend_from_slice(&uncompressed_len.to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&flags.to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&manifest);
        for (_, stored, _, _) in entries {
            out.extend_from_slice(stored);
        }
        out
    }

    /// Builds a minimal native PHAR fixture with uncompressed entries.
    fn build_native_phar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let entries = entries
            .iter()
            .map(|(name, content)| (*name, *content, content.len() as u32, PHAR_FILE_MODE_0644))
            .collect::<Vec<_>>();
        build_native_phar_with_flags(&entries)
    }

    /// Builds a raw-DEFLATE payload for PHAR gzip entry fixtures.
    fn deflate_payload(content: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
    }

    /// Builds a bzip2 payload for PHAR bzip2 entry fixtures.
    fn bzip2_payload(content: &[u8]) -> Vec<u8> {
        let mut encoder =
            bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
    }

    /// Finds one parsed archive entry payload by name.
    fn entry_payload<'a>(entries: &'a [ArchiveEntry], name: &[u8]) -> Option<&'a [u8]> {
        entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.payload.as_slice())
    }

    /// Builds the serialized entry-name format returned by `entry_names_bytes`.
    fn serialized_names(names: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for name in names {
            out.extend_from_slice(&(name.len() as u64).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
        }
        out
    }

    /// Builds a small tar archive with regular-file entries.
    fn build_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, content) in entries {
            let mut header = [0u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            let size = format!("{:011o}\0", content.len());
            header[124..124 + size.len()].copy_from_slice(size.as_bytes());
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            for byte in &mut header[148..156] {
                *byte = b' ';
            }
            let checksum: u32 = header.iter().map(|&b| b as u32).sum();
            let checksum = format!("{:06o}\0 ", checksum);
            header[148..156].copy_from_slice(checksum.as_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(content);
            out.resize(out.len() + round_up_to_512(content.len()).unwrap() - content.len(), 0);
        }
        out.extend_from_slice(&[0u8; 1024]);
        out
    }

    /// Builds a ZIP archive with central-directory records.
    fn build_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, content, deflate) in entries {
            let local_offset = out.len() as u32;
            let stored = if *deflate {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(content).unwrap();
                encoder.finish().unwrap()
            } else {
                content.to_vec()
            };
            let method = if *deflate { ZIP_METHOD_DEFLATE } else { ZIP_METHOD_STORE };
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&stored);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            central.extend_from_slice(&(content.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let central_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// Verifies native PHAR manifest extraction.
    #[test]
    fn extracts_native_phar_entry() {
        let archive = build_native_phar(&[("a.txt", b"alpha"), ("dir/b.txt", b"bravo")]);
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/b.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies tar container extraction.
    #[test]
    fn extracts_tar_entry() {
        let archive = build_tar(&[("a.txt", b"alpha"), ("dir/b.txt", b"bravo")]);
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/b.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies ZIP store and deflate extraction.
    #[test]
    fn extracts_zip_entries() {
        let archive = build_zip(&[
            ("plain.txt", b"stored", false),
            ("deflated.txt", b"deflated payload", true),
        ]);
        assert_eq!(
            extract_entry_bytes(&archive, b"plain.txt").as_deref(),
            Some(&b"stored"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"deflated.txt").as_deref(),
            Some(&b"deflated payload"[..])
        );
    }

    /// Builds a single-entry ZIP whose local header uses a streaming data
    /// descriptor (general-purpose flag bit 3): the local CRC/size fields are
    /// zero, the real values live in a trailing data descriptor, and the central
    /// directory carries the authoritative sizes.
    fn build_zip_with_data_descriptor(name: &str, content: &[u8], deflate: bool) -> Vec<u8> {
        let stored = if deflate {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(content).unwrap();
            encoder.finish().unwrap()
        } else {
            content.to_vec()
        };
        let method = if deflate { ZIP_METHOD_DEFLATE } else { ZIP_METHOD_STORE };
        let crc = crc32(content);
        let comp = stored.len() as u32;
        let uncomp = content.len() as u32;
        let mut out = Vec::new();
        // -- local file header: zeroed sizes, data-descriptor flag set --
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0008u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&stored);
        // -- trailing data descriptor carrying the real crc/sizes --
        out.extend_from_slice(&0x0807_4b50u32.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&comp.to_le_bytes());
        out.extend_from_slice(&uncomp.to_le_bytes());
        // -- central directory with authoritative sizes --
        let central_offset = out.len() as u32;
        let mut central = Vec::new();
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0008u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&comp.to_le_bytes());
        central.extend_from_slice(&uncomp.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&central);
        // -- end of central directory --
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// Verifies a ZIP entry written with a streaming data descriptor (flag bit 3)
    /// is read via the authoritative central-directory sizes instead of rejected,
    /// for both stored and deflated payloads.
    #[test]
    fn extracts_zip_entry_with_data_descriptor() {
        let stored = build_zip_with_data_descriptor("stream.txt", b"streamed payload", false);
        assert_eq!(
            extract_entry_bytes(&stored, b"stream.txt").as_deref(),
            Some(&b"streamed payload"[..])
        );
        let deflated =
            build_zip_with_data_descriptor("stream.txt", b"streamed deflated payload", true);
        assert_eq!(
            extract_entry_bytes(&deflated, b"stream.txt").as_deref(),
            Some(&b"streamed deflated payload"[..])
        );
    }

    /// The ZIP64 extra-field builders emit the tag, length, and only the requested
    /// 64-bit fields in APPNOTE order.
    #[test]
    fn builds_zip64_extra_fields() {
        // Local extra always carries both sizes (16-byte body).
        let local = zip64_local_extra(0x1_0000_0001, 0x2_0000_0002);
        assert_eq!(le16(&local, 0), Some(ZIP64_EXTRA_TAG));
        assert_eq!(le16(&local, 2), Some(16));
        assert_eq!(le64(&local, 4), Some(0x1_0000_0001));
        assert_eq!(le64(&local, 12), Some(0x2_0000_0002));
        // Central extra carries only the overflowed fields, in order.
        let central = zip64_central_extra(Some(7), None, Some(9));
        assert_eq!(le16(&central, 2), Some(16));
        assert_eq!(le64(&central, 4), Some(7));
        assert_eq!(le64(&central, 12), Some(9));
        assert!(zip64_central_extra(None, None, None).len() == 4);
    }

    /// Builds a single-entry ZIP that uses every ZIP64 read path: a central record
    /// whose sizes and header offset are 0xFFFFFFFF sentinels resolved by a ZIP64
    /// extra field, plus a ZIP64 EOCD record + locator behind a sentinel EOCD.
    fn build_zip64_sentinel_fixture(name: &str, content: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let local_offset = out.len() as u32;
        let crc = crc32(content);
        let len = content.len() as u32;
        // -- local header with real sizes (central drives sizes anyway) --
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&45u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(content);
        // -- central record: all three size/offset fields are sentinels --
        let central_offset = out.len();
        let extra = zip64_central_extra(Some(len as u64), Some(len as u64), Some(local_offset as u64));
        let mut central = Vec::new();
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&45u16.to_le_bytes());
        central.extend_from_slice(&45u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
        central.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
        central.extend_from_slice(&extra);
        let central_len = central.len();
        out.extend_from_slice(&central);
        // -- ZIP64 EOCD record + locator --
        let eocd64_offset = out.len() as u64;
        out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
        out.extend_from_slice(&44u64.to_le_bytes());
        out.extend_from_slice(&45u16.to_le_bytes());
        out.extend_from_slice(&45u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&(central_len as u64).to_le_bytes());
        out.extend_from_slice(&(central_offset as u64).to_le_bytes());
        out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&eocd64_offset.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        // -- regular EOCD with count/offset sentinels --
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&ZIP16_SENTINEL.to_le_bytes());
        out.extend_from_slice(&ZIP16_SENTINEL.to_le_bytes());
        out.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
        out.extend_from_slice(&ZIP32_SENTINEL.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// A ZIP64 archive (sentinel central fields + extra field + EOCD64/locator) is
    /// read by resolving the 64-bit values, not rejected.
    #[test]
    fn reads_zip64_archive_with_sentinels() {
        let archive = build_zip64_sentinel_fixture("big.txt", b"zip64 payload body");
        assert_eq!(
            extract_entry_bytes(&archive, b"big.txt").as_deref(),
            Some(&b"zip64 payload body"[..])
        );
    }

    /// Writing more than 65535 entries triggers ZIP64 output (EOCD64 record +
    /// locator), and the bridge reads its own ZIP64 archive back. Set
    /// `ELEPHC_KEEP_ZIP64=<path>` to also dump the archive for an external check.
    #[test]
    fn writes_and_reads_zip64_many_entries() {
        let count = 70_000usize;
        let entries: Vec<ArchiveEntry> = (0..count)
            .map(|i| ArchiveEntry {
                name: format!("f{i}.txt").into_bytes(),
                payload: b"x".to_vec(),
                compression: PharCompression::None,
                metadata: Vec::new(),
            })
            .collect();
        let archive = build_zip_archive(&entries, &[], &[]).unwrap();
        // The ZIP64 EOCD record and locator must be present.
        assert!(find_subslice(&archive, &0x0606_4b50u32.to_le_bytes()).is_some());
        assert!(find_subslice(&archive, &0x0706_4b50u32.to_le_bytes()).is_some());
        // The regular EOCD carries the count sentinel.
        let eocd = find_zip_eocd(&archive).unwrap();
        assert_eq!(le16(&archive, eocd + 10), Some(ZIP16_SENTINEL));
        // Round-trip: the bridge reads back all entries and a sampled payload.
        let parsed = parse_zip_archive(&archive).unwrap();
        assert_eq!(parsed.entries.len(), count);
        assert_eq!(
            extract_entry_bytes(&archive, b"f69999.txt").as_deref(),
            Some(&b"x"[..])
        );
        if let Some(path) = std::env::var_os("ELEPHC_KEEP_ZIP64") {
            std::fs::write(path, &archive).unwrap();
        }
    }

    /// Returns the general-purpose bit-flag field of the first local file header
    /// whose name matches `name`, found by scanning for the local-header signature.
    /// Used to assert that the writer set (or cleared) the ZipCrypto "encrypted"
    /// flag bit on a given entry.
    fn zip_local_flag(archive: &[u8], name: &[u8]) -> Option<u16> {
        let sig = 0x0403_4b50u32.to_le_bytes();
        let mut i = 0;
        while i + 30 <= archive.len() {
            if archive[i..i + 4] == sig {
                let flag = u16::from_le_bytes([archive[i + 6], archive[i + 7]]);
                let name_len = u16::from_le_bytes([archive[i + 26], archive[i + 27]]) as usize;
                let name_start = i + 30;
                if archive.get(name_start..name_start + name_len) == Some(name) {
                    return Some(flag);
                }
            }
            i += 1;
        }
        None
    }

    /// Builds a single-entry ZIP whose stored entry is traditional-PKWARE
    /// (ZipCrypto) encrypted with `password`: a 12-byte encryption header (last
    /// byte = the CRC's high byte check) plus the encrypted payload.
    fn build_zipcrypto_zip(name: &str, content: &[u8], password: &[u8]) -> Vec<u8> {
        let crc = crc32(content);
        // Reuse the production encryptor so the test fixture and the writer share a
        // single cipher direction (check byte = the CRC's high byte, no descriptor).
        let enc = zipcrypto_encrypt(password, content, (crc >> 24) as u8);
        let csz = enc.len() as u32;
        let usz = content.len() as u32;
        let mut out = Vec::new();
        // -- local header with the encrypted flag set --
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&ZIP_FLAG_ENCRYPTED.to_le_bytes());
        out.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&csz.to_le_bytes());
        out.extend_from_slice(&usz.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&enc);
        // -- central record --
        let central_offset = out.len() as u32;
        let mut central = Vec::new();
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&ZIP_FLAG_ENCRYPTED.to_le_bytes());
        central.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&csz.to_le_bytes());
        central.extend_from_slice(&usz.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes()); // local header offset
        central.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&central);
        // -- end of central directory --
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// A ZipCrypto-encrypted ZIP entry decrypts only with the correct password set
    /// via `set_zip_password`; a missing or wrong password yields no payload.
    #[test]
    fn reads_zipcrypto_encrypted_entry() {
        let content = b"secret zipcrypto payload\n";
        let archive = build_zipcrypto_zip("zc.txt", content, b"hunter2");
        // No password set: the encrypted entry is unreadable.
        set_zip_password(b"");
        assert_eq!(extract_entry_bytes(&archive, b"zc.txt"), None);
        // Wrong password is rejected by the header check byte.
        set_zip_password(b"wrong-password");
        assert_eq!(extract_entry_bytes(&archive, b"zc.txt"), None);
        // Correct password decrypts the entry.
        set_zip_password(b"hunter2");
        assert_eq!(
            extract_entry_bytes(&archive, b"zc.txt").as_deref(),
            Some(&content[..])
        );
        set_zip_password(b"");
    }

    /// With a zip password set, `build_zip_archive` encrypts every file entry — the
    /// stub included — so entries read back only with the correct password; a wrong
    /// or cleared password fails, and an archive built with no password stays plain.
    #[test]
    fn writes_then_reads_zipcrypto_entry() {
        let stored = b"plain stored payload".to_vec();
        // Repetitive bytes so the deflate path actually compresses.
        let deflated = b"compress me ".repeat(64);
        let entries = vec![
            ArchiveEntry {
                name: b"a.txt".to_vec(),
                payload: stored.clone(),
                compression: PharCompression::None,
                metadata: Vec::new(),
            },
            ArchiveEntry {
                name: b"b.txt".to_vec(),
                payload: deflated.clone(),
                compression: PharCompression::Gzip,
                metadata: Vec::new(),
            },
        ];
        let stub = PHAR_DEFAULT_STUB.to_vec();

        set_zip_password(b"hunter2");
        let archive = build_zip_archive(&entries, &[], &stub).unwrap();

        // The correct password decrypts both the stored and the deflated entry.
        assert_eq!(
            extract_entry_bytes(&archive, b"a.txt").as_deref(),
            Some(&stored[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"b.txt").as_deref(),
            Some(&deflated[..])
        );

        // The encrypted flag is set on a regular entry and on the stub (chosen scope).
        assert_eq!(zip_local_flag(&archive, b"a.txt"), Some(ZIP_FLAG_ENCRYPTED));
        assert_eq!(zip_local_flag(&archive, PHAR_STUB_ENTRY), Some(ZIP_FLAG_ENCRYPTED));

        // A wrong then cleared password cannot decrypt the entry.
        set_zip_password(b"nope");
        assert_eq!(extract_entry_bytes(&archive, b"a.txt"), None);
        set_zip_password(b"");
        assert_eq!(extract_entry_bytes(&archive, b"a.txt"), None);

        // Built with no password the archive is plain and reads with none set.
        let plain = build_zip_archive(&entries, &[], &stub).unwrap();
        assert_eq!(
            extract_entry_bytes(&plain, b"a.txt").as_deref(),
            Some(&stored[..])
        );
        assert_eq!(zip_local_flag(&plain, b"a.txt"), Some(0));
    }

    /// Signing a zip phar whose entries are encrypted still produces a readable
    /// `.phar/signature.bin`: the signed range covers the encrypted bytes, the entry
    /// decrypts with the password, the signature reports SHA-256, and the signature
    /// entry itself stays in the clear (no encrypted flag).
    #[test]
    fn signed_encrypted_zip_still_verifies() {
        let path =
            std::env::temp_dir().join(format!("elephc_phar_encsig_{}.zip", std::process::id()));
        let pb = path.to_string_lossy();

        set_zip_password(b"hunter2");
        // Write an encrypted entry, then SHA-256 (algo 3) sign the archive.
        assert_eq!(
            put_entry_bytes(pb.as_bytes(), b"doc.txt", b"top secret\n"),
            Some(11)
        );
        assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));
        let data = std::fs::read(&path).unwrap();

        // The entry still decrypts; the signature reports SHA-256 with a 32-byte digest.
        assert_eq!(
            extract_entry_bytes(&data, b"doc.txt").as_deref(),
            Some(&b"top secret\n"[..])
        );
        assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"SHA-256"[..]));
        let (flag, digest) = read_signature_info(pb.as_bytes()).unwrap();
        assert_eq!(flag, 3);
        assert_eq!(digest.len(), 32);

        // The entry is encrypted but the signature entry stays in the clear.
        assert_eq!(zip_local_flag(&data, b"doc.txt"), Some(ZIP_FLAG_ENCRYPTED));
        assert_eq!(zip_local_flag(&data, PHAR_SIGNATURE_ENTRY), Some(0));

        // Without the password the encrypted entry is unreadable.
        set_zip_password(b"");
        assert_eq!(extract_entry_bytes(&data, b"doc.txt"), None);
        std::fs::remove_file(&path).ok();
    }

    /// Verifies entry-name listing across supported archive families.
    #[test]
    fn lists_entry_names_for_supported_archive_families() {
        let base = std::env::temp_dir().join(format!(
            "elephc_phar_list_{}_{}",
            std::process::id(),
            "unit"
        ));
        let phar_path = base.with_extension("phar");
        let tar_path = base.with_extension("tar");
        let zip_path = base.with_extension("zip");

        std::fs::write(
            &phar_path,
            build_native_phar(&[("one.txt", b"alpha"), ("dir/two.txt", b"bravo")]),
        )
        .unwrap();
        std::fs::write(
            &tar_path,
            build_tar(&[("tar.txt", b"tar"), ("dir/nested.txt", b"nested")]),
        )
        .unwrap();
        std::fs::write(
            &zip_path,
            build_zip(&[("zip.txt", b"zip", false), ("def.txt", b"def", true)]),
        )
        .unwrap();

        assert_eq!(
            entry_names_bytes(phar_path.to_string_lossy().as_bytes()).as_deref(),
            Some(serialized_names(&["one.txt", "dir/two.txt"]).as_slice())
        );
        assert_eq!(
            entry_names_bytes(tar_path.to_string_lossy().as_bytes()).as_deref(),
            Some(serialized_names(&["tar.txt", "dir/nested.txt"]).as_slice())
        );
        assert_eq!(
            entry_names_bytes(zip_path.to_string_lossy().as_bytes()).as_deref(),
            Some(serialized_names(&["zip.txt", "def.txt"]).as_slice())
        );

        std::fs::remove_file(&phar_path).ok();
        std::fs::remove_file(&tar_path).ok();
        std::fs::remove_file(&zip_path).ok();
    }

    /// Verifies native PHAR writes preserve existing entries and update duplicates.
    #[test]
    fn writes_and_updates_native_phar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_put_entry_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"updated"),
            Some(7)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"updated"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies native PHAR writes preserve gzip compression on replaced entries.
    #[test]
    fn writes_preserve_gzip_native_phar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_gzip_update_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let original = b"gzip old payload gzip old payload";
        let stored = deflate_payload(original);
        let archive = build_native_phar_with_flags(&[(
            "z.txt",
            &stored,
            original.len() as u32,
            PHAR_FILE_MODE_0644 | PHAR_FLAG_GZIP,
        )]);
        std::fs::write(&path, archive).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"z.txt", b"gzip updated payload"),
            Some(20)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let entries = parse_native_phar_archive(&archive).unwrap().entries;
        assert_eq!(entries[0].compression, PharCompression::Gzip);
        assert_eq!(entries[0].payload, b"gzip updated payload");
    }

    /// Verifies native PHAR writes preserve bzip2 compression on replaced entries.
    #[test]
    fn writes_preserve_bzip2_native_phar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_bzip2_update_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let original = b"bzip2 old payload bzip2 old payload";
        let stored = bzip2_payload(original);
        let archive = build_native_phar_with_flags(&[(
            "b.txt",
            &stored,
            original.len() as u32,
            PHAR_FILE_MODE_0644 | PHAR_FLAG_BZIP2,
        )]);
        std::fs::write(&path, archive).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"b.txt", b"bzip2 updated payload"),
            Some(21)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let entries = parse_native_phar_archive(&archive).unwrap().entries;
        assert_eq!(entries[0].compression, PharCompression::Bzip2);
        assert_eq!(entries[0].payload, b"bzip2 updated payload");
    }

    /// Verifies buffered PHAR stream descriptors keep concurrent payloads separate.
    #[test]
    fn concurrent_phar_write_streams_preserve_distinct_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_streams_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let path_bytes = path.to_string_lossy();
        let path_raw = path_bytes.as_bytes();
        let one = b"one.txt";
        let two = b"two.txt";
        let fd_one = unsafe {
            elephc_phar_stream_open_entry(path_raw.as_ptr(), path_raw.len(), one.as_ptr(), one.len())
        };
        let fd_two = unsafe {
            elephc_phar_stream_open_entry(path_raw.as_ptr(), path_raw.len(), two.as_ptr(), two.len())
        };
        assert_ne!(fd_one, usize::MAX);
        assert_ne!(fd_two, usize::MAX);
        assert_ne!(fd_one, fd_two);
        assert_eq!(
            unsafe { elephc_phar_stream_append(fd_two, b"bravo".as_ptr(), 5) },
            5
        );
        assert_eq!(
            unsafe { elephc_phar_stream_append(fd_one, b"alpha".as_ptr(), 5) },
            5
        );
        assert_eq!(elephc_phar_stream_finalize(fd_one), 1);
        assert_eq!(elephc_phar_stream_finalize(fd_two), 1);
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let entries = parse_native_phar_archive(&archive).unwrap().entries;
        assert_eq!(entry_payload(&entries, b"one.txt"), Some(b"alpha".as_slice()));
        assert_eq!(entry_payload(&entries, b"two.txt"), Some(b"bravo".as_slice()));
    }

    /// Verifies tar writes preserve the tar container family while updating entries.
    #[test]
    fn writes_tar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_tar_write_{}_{}.tar",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&path, build_tar(&[("one.txt", b"alpha")])).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"alpha"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
        assert_ne!(archive.get(0..5), Some(&b"<?php"[..]));
    }

    /// Verifies ZIP writes preserve the ZIP container family while updating entries.
    #[test]
    fn writes_zip_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_zip_write_{}_{}.zip",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&path, build_zip(&[("one.txt", b"alpha", true)])).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(archive.get(0..4), Some(&[0x50, 0x4b, 0x03, 0x04][..]));
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"alpha"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies native PHAR deletion removes one entry while preserving siblings.
    #[test]
    fn deletes_native_phar_entry_from_url() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_delete_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"two.txt", b"bravo"),
            Some(5)
        );
        let url = format!("phar://{}/one.txt", path.display());
        assert_eq!(delete_url_bytes(url.as_bytes()), Some(()));
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(extract_entry_bytes(&archive, b"one.txt"), None);
        assert_eq!(
            extract_entry_bytes(&archive, b"two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies tar and ZIP deletion preserve the archive family.
    #[test]
    fn deletes_tar_and_zip_entries() {
        let tar_path = std::env::temp_dir().join(format!(
            "elephc_phar_delete_{}_{}.tar",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&tar_path, build_tar(&[("one.txt", b"alpha"), ("two.txt", b"bravo")]))
            .unwrap();
        let tar_url = format!("phar://{}/one.txt", tar_path.display());
        assert_eq!(delete_url_bytes(tar_url.as_bytes()), Some(()));
        let tar_archive = std::fs::read(&tar_path).unwrap();
        std::fs::remove_file(&tar_path).ok();
        assert_eq!(extract_entry_bytes(&tar_archive, b"one.txt"), None);
        assert_eq!(
            extract_entry_bytes(&tar_archive, b"two.txt").as_deref(),
            Some(&b"bravo"[..])
        );

        let zip_path = std::env::temp_dir().join(format!(
            "elephc_phar_delete_{}_{}.zip",
            std::process::id(),
            "unit"
        ));
        std::fs::write(
            &zip_path,
            build_zip(&[("one.txt", b"alpha", false), ("two.txt", b"bravo", true)]),
        )
        .unwrap();
        let zip_url = format!("phar://{}/one.txt", zip_path.display());
        assert_eq!(delete_url_bytes(zip_url.as_bytes()), Some(()));
        let zip_archive = std::fs::read(&zip_path).unwrap();
        std::fs::remove_file(&zip_path).ok();
        assert_eq!(zip_archive.get(0..4), Some(&[0x50, 0x4b, 0x03, 0x04][..]));
        assert_eq!(extract_entry_bytes(&zip_archive, b"one.txt"), None);
        assert_eq!(
            extract_entry_bytes(&zip_archive, b"two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies deletion fails cleanly when the requested entry is absent.
    #[test]
    fn delete_missing_entry_returns_none() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_delete_missing_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        let url = format!("phar://{}/missing.txt", path.display());
        assert_eq!(delete_url_bytes(url.as_bytes()), None);
        std::fs::remove_file(&path).ok();
    }

    /// Verifies native PHAR archive-wide compression controls rewrite all entries.
    #[test]
    fn sets_native_phar_archive_compression() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_compress_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"two.txt", b"bravo"),
            Some(5)
        );
        assert_eq!(set_archive_compression(path_bytes.as_bytes(), 4_096), Some(()));
        let gzip_archive = std::fs::read(&path).unwrap();
        let gzip_entries = parse_native_phar_archive(&gzip_archive).unwrap().entries;
        assert!(gzip_entries
            .iter()
            .all(|entry| entry.compression == PharCompression::Gzip));
        assert_eq!(
            extract_entry_bytes(&gzip_archive, b"two.txt").as_deref(),
            Some(&b"bravo"[..])
        );

        assert_eq!(set_archive_compression(path_bytes.as_bytes(), 0), Some(()));
        let plain_archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let plain_entries = parse_native_phar_archive(&plain_archive).unwrap().entries;
        assert!(plain_entries
            .iter()
            .all(|entry| entry.compression == PharCompression::None));
        assert_eq!(
            extract_entry_bytes(&plain_archive, b"one.txt").as_deref(),
            Some(&b"alpha"[..])
        );
    }

    /// Verifies ZIP archive compression controls rewrite stored and deflated entries.
    #[test]
    fn sets_zip_archive_compression() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_zip_compress_{}_{}.zip",
            std::process::id(),
            "unit"
        ));
        std::fs::write(
            &path,
            build_zip(&[
                ("one.txt", b"alpha alpha alpha", false),
                ("two.txt", b"bravo bravo bravo", false),
            ]),
        )
        .unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(set_archive_compression(path_bytes.as_bytes(), 4_096), Some(()));
        let deflated_archive = std::fs::read(&path).unwrap();
        let deflated_entries = parse_zip_archive(&deflated_archive).unwrap().entries;
        assert!(deflated_entries
            .iter()
            .all(|entry| entry.compression == PharCompression::Gzip));
        assert_eq!(
            extract_entry_bytes(&deflated_archive, b"two.txt").as_deref(),
            Some(&b"bravo bravo bravo"[..])
        );

        assert_eq!(set_archive_compression(path_bytes.as_bytes(), 0), Some(()));
        let stored_archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let stored_entries = parse_zip_archive(&stored_archive).unwrap().entries;
        assert!(stored_entries
            .iter()
            .all(|entry| entry.compression == PharCompression::None));
        assert_eq!(
            extract_entry_bytes(&stored_archive, b"one.txt").as_deref(),
            Some(&b"alpha alpha alpha"[..])
        );
    }

    /// Verifies compression controls reject unsupported constants and containers.
    #[test]
    fn set_compression_rejects_unsupported_inputs() {
        let phar_path = std::env::temp_dir().join(format!(
            "elephc_phar_compress_bad_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let phar_bytes = phar_path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(phar_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(set_archive_compression(phar_bytes.as_bytes(), 123), None);
        std::fs::remove_file(&phar_path).ok();

        let tar_path = std::env::temp_dir().join(format!(
            "elephc_phar_compress_bad_{}_{}.tar",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&tar_path, build_tar(&[("one.txt", b"alpha")])).unwrap();
        let tar_bytes = tar_path.to_string_lossy();
        assert_eq!(set_archive_compression(tar_bytes.as_bytes(), 4_096), None);
        std::fs::remove_file(&tar_path).ok();

        let zip_path = std::env::temp_dir().join(format!(
            "elephc_phar_compress_bad_{}_{}.zip",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&zip_path, build_zip(&[("one.txt", b"alpha", false)])).unwrap();
        let zip_bytes = zip_path.to_string_lossy();
        assert_eq!(set_archive_compression(zip_bytes.as_bytes(), 8_192), None);
        std::fs::remove_file(&zip_path).ok();
    }

    /// Verifies full phar:// URL writes split archive and entry names at run time.
    #[test]
    fn writes_native_phar_entries_from_url() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_put_url_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let url = format!("phar://{}/one.txt", path.display());
        assert_eq!(put_url_bytes(url.as_bytes(), b"alpha"), Some(5));
        let nested_url = format!("phar://{}/dir/two.txt", path.display());
        assert_eq!(put_url_bytes(nested_url.as_bytes(), b"bravo"), Some(5));
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"alpha"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Stub used by the metadata/stub round-trip tests; ends with `?>\r\n` so the
    /// native-PHAR `__HALT_COMPILER();` boundary scan round-trips it exactly.
    const ROUND_TRIP_STUB: &[u8] = b"<?php Phar::mapPhar(); __HALT_COMPILER(); ?>\r\n";
    const ROUND_TRIP_META: &[u8] = b"a:1:{s:3:\"ver\";s:5:\"1.2.3\";}";

    /// Shared body: set metadata+stub, prove they survive a later entry write, and
    /// that the reserved `.phar/*` control files stay hidden from the entry listing.
    fn check_metadata_stub_round_trip(ext: &str, tag: &str) {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_meta_{}_{}.{}",
            std::process::id(),
            tag,
            ext
        ));
        let pb = path.to_string_lossy();
        let pbytes = pb.as_bytes();
        assert_eq!(put_entry_bytes(pbytes, b"a.txt", b"alpha"), Some(5));
        assert_eq!(set_metadata_bytes(pbytes, ROUND_TRIP_META), Some(()));
        assert_eq!(set_stub_bytes(pbytes, ROUND_TRIP_STUB), Some(()));
        // A later entry write must preserve both metadata and stub.
        assert_eq!(put_entry_bytes(pbytes, b"b.txt", b"bravo"), Some(5));
        assert_eq!(get_metadata_bytes(pbytes).as_deref(), Some(ROUND_TRIP_META));
        assert_eq!(get_stub_bytes(pbytes).as_deref(), Some(ROUND_TRIP_STUB));
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            extract_entry_bytes(&archive, b"a.txt").as_deref(),
            Some(&b"alpha"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"b.txt").as_deref(),
            Some(&b"bravo"[..])
        );
        let (entries, _) = parse_archive_entries(&archive).unwrap();
        assert_eq!(entries.len(), 2, "{} entry count", tag);
        assert!(
            entries.iter().all(|e| !e.name.starts_with(b".phar/")),
            "{} leaked a .phar/ control entry",
            tag
        );
    }

    const ROUND_TRIP_FILE_META: &[u8] = b"a:1:{s:4:\"role\";s:5:\"first\";}";

    /// Drives a per-file metadata round-trip for one archive family: set metadata on
    /// one entry, confirm it survives a later entry write, and that only the targeted
    /// entry carries metadata while `.phar/` control entries never leak.
    fn check_file_metadata_round_trip(ext: &str, tag: &str) {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_filemeta_{}_{}.{}",
            std::process::id(),
            tag,
            ext
        ));
        let pb = path.to_string_lossy();
        let pbytes = pb.as_bytes();
        assert_eq!(put_entry_bytes(pbytes, b"a.txt", b"alpha"), Some(5));
        assert_eq!(put_entry_bytes(pbytes, b"b.txt", b"bravo"), Some(5));
        assert_eq!(
            set_file_metadata_bytes(pbytes, b"a.txt", ROUND_TRIP_FILE_META),
            Some(())
        );
        // A later entry write must preserve the per-file metadata.
        assert_eq!(put_entry_bytes(pbytes, b"c.txt", b"charlie"), Some(7));
        assert_eq!(
            get_file_metadata_bytes(pbytes, b"a.txt").as_deref(),
            Some(ROUND_TRIP_FILE_META),
            "{} a.txt metadata",
            tag
        );
        // Untouched entries carry no metadata.
        assert_eq!(
            get_file_metadata_bytes(pbytes, b"b.txt").as_deref(),
            Some(&b""[..]),
            "{} b.txt metadata",
            tag
        );
        // Setting metadata on a missing entry fails.
        assert_eq!(
            set_file_metadata_bytes(pbytes, b"missing.txt", ROUND_TRIP_FILE_META),
            None
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let (entries, _) = parse_archive_entries(&archive).unwrap();
        assert_eq!(entries.len(), 3, "{} entry count", tag);
        assert!(
            entries.iter().all(|e| !e.name.starts_with(b".phar/")),
            "{} leaked a .phar/ control entry",
            tag
        );
    }

    /// Drives a whole-archive compression round-trip: build a tar, compress it with
    /// `compressor`, confirm the returned compressed file parses transparently with
    /// entries intact, then decompress it and confirm the entries survive again.
    fn check_archive_compression_round_trip(
        tag: &str,
        ext: &str,
        compressor: fn(&[u8]) -> Option<Vec<u8>>,
    ) {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("elephc_phar_comp_{}_{}.tar", std::process::id(), tag));
        let sb = src.to_string_lossy();
        assert_eq!(put_entry_bytes(sb.as_bytes(), b"a.txt", b"alpha"), Some(5));
        assert_eq!(put_entry_bytes(sb.as_bytes(), b"b.txt", b"bravo"), Some(5));
        let comp_bytes = compressor(sb.as_bytes()).expect("compress");
        let comp = String::from_utf8(comp_bytes).unwrap();
        assert_eq!(comp, format!("{}.{}", sb, ext), "{} dest path", tag);
        // The compressed file parses transparently with entries intact.
        let (entries, _) = parse_archive_entries(&std::fs::read(&comp).unwrap()).unwrap();
        assert_eq!(entries.len(), 2, "{} compressed entry count", tag);
        assert_eq!(
            extract_entry_bytes(&std::fs::read(&comp).unwrap(), b"a.txt"),
            Some(b"alpha".to_vec())
        );
        // Decompressing reproduces a plain tar (the `.tar` base) with the same entries.
        let back_bytes = decompress_archive(comp.as_bytes()).expect("decompress");
        let back = String::from_utf8(back_bytes).unwrap();
        assert_eq!(back, sb.to_string(), "{} decompress dest path", tag);
        let plain = std::fs::read(&back).unwrap();
        assert_eq!(plain.get(257..262), Some(&b"ustar"[..]), "{} decompressed is tar", tag);
        assert_eq!(extract_entry_bytes(&plain, b"b.txt"), Some(b"bravo".to_vec()));
        for p in [src.to_string_lossy().to_string(), comp] {
            std::fs::remove_file(p).ok();
        }
    }

    /// A tar archive round-trips through whole-archive gzip compression.
    #[test]
    fn tar_archive_gzip_compress_round_trip() {
        check_archive_compression_round_trip("gz", "gz", gzip_archive);
    }

    /// A tar archive round-trips through whole-archive bzip2 compression.
    #[test]
    fn tar_archive_bzip2_compress_round_trip() {
        check_archive_compression_round_trip("bz", "bz2", bzip2_archive);
    }

    const TEST_RSA_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAOuAP7xZaVfhwn9l\n\
BaMgxKPU1ODBpuT7Ybu6Fav03TJp1BKc1wUMiXnUPraUUI2R2JxoattDe7R/LcGk\n\
jVoPiBGGPoxxTaByd5LJZJk6MJAiGBhzQT7bkK3OMDHLQqhziefqDFfnDLt/TN7+\n\
umuMCPtLmuF6UUXiebMzyH21x7jvAgMBAAECgYBBhL+2rgVxzrxm5vsnhEFQ9zB2\n\
i0ncYNey+7V1zr0PfoPi3cGwhOlmfJcqAp9ak534/c/kyqSK9esL+bTdvn5zIQqC\n\
Swt2znffaW9nC6lM/pkZcvGLETt2m0L71n6pZVkMewsGBm9YrBQFA1krC7BV674U\n\
mlOmmYpM3LPgzmRLwQJBAPm/G7O4Stmzu5xV5qtvYX1dNZ2gydkVyfK/AwCYpfbK\n\
8ZXntKeWCt1BER1hNBSMPacHKb0LotK3j3LNNteLHCECQQDxZdNsXNLTHylWKA/X\n\
dyM3SH9mM6ESZP07cU7Ifq6t9zJdTfGdiyxsAjaaXxDmShL+bAjU16iwaTAGcYTB\n\
NrMPAkEAoUGwVV7Nlbvji5I7mr4UKKoikGDdc/oJp1+GRMBLiQqI6s3ta7gJ08rL\n\
jjjRM+NJe6u4W4RD4eL8EJhIrOv5gQJAK4Tm+8c0PtmEU0L/sCGLWMEaLquqIy3P\n\
tXK0+FJWXYiOLOILaBKaHJK9k1EGM+4wxGtnoC+M+tjLzq2SeF7LIwJAPdLUn2Qq\n\
eGMK12chOVcx41RxYctqsOlEKCIt011yGsV2/Mdm9ljTXeyXvNXCVOVcnHaf1v5w\n\
rNiobfy8sSb6iw==\n\
-----END PRIVATE KEY-----\n";

    /// OpenSSL signing replaces the native PHAR's SHA1 trailer with an RSA-SHA1
    /// signature trailer, the signature is deterministic and verifies against the
    /// derived public key, and the signature metadata reads back as OpenSSL.
    #[test]
    fn native_phar_openssl_signature_round_trip() {
        use rsa::pkcs8::DecodePrivateKey;
        use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
        use sha1::{Digest, Sha1};

        let path = std::env::temp_dir().join(format!("elephc_phar_sig_{}.phar", std::process::id()));
        let pb = path.to_string_lossy();
        assert_eq!(put_entry_bytes(pb.as_bytes(), b"a.txt", b"alpha"), Some(5));
        assert_eq!(
            sign_archive_openssl(pb.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
            Some(())
        );

        let signed = std::fs::read(&path).unwrap();
        let n = signed.len();
        assert_eq!(&signed[n - 4..], b"GBMB");
        assert_eq!(
            u32::from_le_bytes(signed[n - 8..n - 4].try_into().unwrap()),
            PHAR_OPENSSL_SIGNATURE_TYPE
        );

        // The signature reads back as OpenSSL with a 1024-bit (128-byte) RSA signature.
        let (flags, sig) = read_signature_info(pb.as_bytes()).unwrap();
        assert_eq!(flags, PHAR_OPENSSL_SIGNATURE_TYPE);
        assert_eq!(sig.len(), 128, "1024-bit RSA signature is 128 bytes");
        assert_eq!(
            u32::from_le_bytes(signed[n - 12..n - 8].try_into().unwrap()) as usize,
            sig.len()
        );
        assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"OpenSSL"[..]));

        // Re-signing is deterministic (PKCS#1 v1.5).
        assert_eq!(
            sign_archive_openssl(pb.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
            Some(())
        );
        let (_, sig2) = read_signature_info(pb.as_bytes()).unwrap();
        assert_eq!(sig, sig2, "PKCS#1 v1.5 signature is deterministic");

        // The signature verifies against the public key over the signed data.
        let key = RsaPrivateKey::from_pkcs8_pem(TEST_RSA_KEY_PEM).unwrap();
        let pubkey = RsaPublicKey::from(&key);
        let data = strip_signature_trailer(&std::fs::read(&path).unwrap()).to_vec();
        let hashed = Sha1::digest(&data);
        pubkey
            .verify(Pkcs1v15Sign::new::<Sha1>(), &hashed, &sig)
            .expect("signature verifies");
        std::fs::remove_file(&path).ok();
    }

    /// Hash-based signing rewrites the native PHAR trailer with the requested digest
    /// algorithm, readable back via the signature metadata.
    #[test]
    fn native_phar_hash_signature_round_trip() {
        let path = std::env::temp_dir().join(format!("elephc_phar_hsig_{}.phar", std::process::id()));
        let pb = path.to_string_lossy();
        assert_eq!(put_entry_bytes(pb.as_bytes(), b"a.txt", b"alpha"), Some(5));
        // SHA-256 (algo 3): 32-byte digest, type "SHA-256".
        assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));
        let (flags, digest) = read_signature_info(pb.as_bytes()).unwrap();
        assert_eq!(flags, 3);
        assert_eq!(digest.len(), 32);
        assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"SHA-256"[..]));
        std::fs::remove_file(&path).ok();
    }

    /// Reconstructs the byte range a tar/zip phar signature is computed over from a
    /// parsed archive: the tar data records, or the zip locals + central + comment.
    fn tar_zip_signed_range(arch: &Archive) -> Vec<u8> {
        match arch.format {
            ArchiveFormat::Tar => {
                let mut body = Vec::new();
                write_tar_body(&mut body, &arch.entries, &arch.metadata, &arch.stub).unwrap();
                body
            }
            ArchiveFormat::Zip => {
                let mut out = Vec::new();
                let mut central = Vec::new();
                write_zip_body(&mut out, &mut central, &arch.entries, &arch.stub).unwrap();
                out.extend_from_slice(&central);
                out.extend_from_slice(&arch.metadata);
                out
            }
            ArchiveFormat::NativePhar => unreachable!("native phars sign with a trailer"),
        }
    }

    /// Hash signing a tar/zip phar writes a hidden `.phar/signature.bin` entry
    /// (`LE32(flag) ++ LE32(len) ++ digest`) computed over the signed range, leaving
    /// real entries readable and reporting the right algorithm.
    fn check_tar_zip_hash_signature(ext: &str) {
        let path =
            std::env::temp_dir().join(format!("elephc_phar_sig_{}_{ext}.{ext}", std::process::id()));
        let pb = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(pb.as_bytes(), b"doc.txt", b"bundled document\n"),
            Some(17)
        );
        // SHA-256 (algo 3).
        assert_eq!(sign_archive_hash(pb.as_bytes(), 3), Some(()));
        let data = std::fs::read(&path).unwrap();
        // The signature entry is hidden; the real entry still reads back.
        assert_eq!(
            extract_entry_bytes(&data, b"doc.txt").as_deref(),
            Some(&b"bundled document\n"[..])
        );
        let (flag, digest) = read_signature_info(pb.as_bytes()).unwrap();
        assert_eq!(flag, 3);
        assert_eq!(digest.len(), 32);
        assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"SHA-256"[..]));
        // The digest covers exactly the reconstructed signed range.
        let arch = parse_archive(&data).unwrap();
        assert_eq!(digest, compute_signature(3, None, &tar_zip_signed_range(&arch)).unwrap());
        std::fs::remove_file(&path).ok();
    }

    /// SHA-256 signing a tar phar round-trips through `.phar/signature.bin`.
    #[test]
    fn tar_phar_hash_signature_round_trip() {
        check_tar_zip_hash_signature("tar");
    }

    /// SHA-256 signing a zip phar round-trips through `.phar/signature.bin`.
    #[test]
    fn zip_phar_hash_signature_round_trip() {
        check_tar_zip_hash_signature("zip");
    }

    /// OpenSSL signing a tar/zip phar writes an RSA-SHA1 `.phar/signature.bin` that
    /// verifies against the derived public key over the archive's signed range.
    fn check_tar_zip_openssl_signature(ext: &str) {
        use rsa::pkcs8::DecodePrivateKey;
        use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
        use sha1::{Digest, Sha1};

        let path =
            std::env::temp_dir().join(format!("elephc_phar_osig_{}_{ext}.{ext}", std::process::id()));
        let pb = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(pb.as_bytes(), b"doc.txt", b"bundled document\n"),
            Some(17)
        );
        assert_eq!(
            sign_archive_openssl(pb.as_bytes(), TEST_RSA_KEY_PEM.as_bytes()),
            Some(())
        );
        let data = std::fs::read(&path).unwrap();
        let (flag, sig) = read_signature_info(pb.as_bytes()).unwrap();
        assert_eq!(flag, PHAR_OPENSSL_SIGNATURE_TYPE);
        assert_eq!(sig.len(), 128, "1024-bit RSA signature is 128 bytes");
        assert_eq!(signature_type_name(pb.as_bytes()).as_deref(), Some(&b"OpenSSL"[..]));
        // The signature verifies against the public key over the signed range.
        let arch = parse_archive(&data).unwrap();
        let key = RsaPrivateKey::from_pkcs8_pem(TEST_RSA_KEY_PEM).unwrap();
        let pubkey = RsaPublicKey::from(&key);
        let hashed = Sha1::digest(tar_zip_signed_range(&arch));
        pubkey
            .verify(Pkcs1v15Sign::new::<Sha1>(), &hashed, &sig)
            .expect("tar/zip OpenSSL signature verifies");
        std::fs::remove_file(&path).ok();
    }

    /// OpenSSL signing a tar phar verifies against the derived public key.
    #[test]
    fn tar_phar_openssl_signature_round_trip() {
        check_tar_zip_openssl_signature("tar");
    }

    /// OpenSSL signing a zip phar verifies against the derived public key.
    #[test]
    fn zip_phar_openssl_signature_round_trip() {
        check_tar_zip_openssl_signature("zip");
    }

    /// Per-file metadata round-trips through the native manifest per-entry field.
    #[test]
    fn native_phar_file_metadata_round_trip() {
        check_file_metadata_round_trip("phar", "native");
    }

    /// Per-file metadata round-trips through `.phar/.metadata/<path>/.metadata.bin`.
    #[test]
    fn tar_phar_file_metadata_round_trip() {
        check_file_metadata_round_trip("tar", "tar");
    }

    /// Per-file metadata round-trips through the zip central-directory file comment.
    #[test]
    fn zip_phar_file_metadata_round_trip() {
        check_file_metadata_round_trip("zip", "zip");
    }

    /// Verifies native-PHAR global metadata and stub persist and survive entry writes.
    #[test]
    fn native_phar_metadata_and_stub_round_trip() {
        check_metadata_stub_round_trip("phar", "native");
    }

    /// Verifies tar-based phar metadata/stub persist via `.phar/.metadata.bin` and
    /// `.phar/stub.php`, and survive entry writes.
    #[test]
    fn tar_phar_metadata_and_stub_round_trip() {
        check_metadata_stub_round_trip("tar", "tar");
    }

    /// Verifies zip-based phar metadata persists in the EOCD comment and the stub in
    /// `.phar/stub.php`, and both survive entry writes.
    #[test]
    fn zip_phar_metadata_and_stub_round_trip() {
        check_metadata_stub_round_trip("zip", "zip");
    }

    /// Verifies `set_stub_bytes` rejects a stub without the `__HALT_COMPILER();` marker.
    #[test]
    fn set_stub_requires_halt_compiler() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_badstub_{}.phar",
            std::process::id()
        ));
        let pb = path.to_string_lossy();
        assert_eq!(put_entry_bytes(pb.as_bytes(), b"a.txt", b"alpha"), Some(5));
        assert_eq!(set_stub_bytes(pb.as_bytes(), b"<?php echo 1;"), None);
        std::fs::remove_file(&path).ok();
    }
}
