//! Purpose:
//! Shared PHAR archive model, constants, buffers, and buffered-stream state.
//!
//! Called from:
//! - The focused PHAR parser, writer, FFI, and stream modules.
//!
//! Key details:
//! - Archive-family and compression metadata must survive read-modify-write cycles.

use super::*;

pub(super) const PHAR_FLAG_GZIP: u32 = 0x0000_1000;
pub(super) const PHAR_FLAG_BZIP2: u32 = 0x0000_2000;
pub(super) const PHAR_HDR_SIGNATURE: u32 = 0x0001_0000;
pub(super) const PHAR_FILE_MODE_0644: u32 = 0x0000_01a4;
pub(super) const PHAR_SHA1_SIGNATURE_TYPE: u32 = 0x0000_0002;
pub(super) const PHAR_OPENSSL_SIGNATURE_TYPE: u32 = 0x0000_0010;
pub(super) const ZIP_METHOD_STORE: u16 = 0;
pub(super) const ZIP_METHOD_DEFLATE: u16 = 8;
/// ZIP general-purpose flag bit 0: the entry is encrypted (traditional ZipCrypto).
pub(super) const ZIP_FLAG_ENCRYPTED: u16 = 0x0001;
/// ZIP general-purpose flag bit 3: sizes/CRC are in a trailing data descriptor.
pub(super) const ZIP_FLAG_DATA_DESCRIPTOR: u16 = 0x0008;
/// ZIP64 extended-information extra-field tag.
pub(super) const ZIP64_EXTRA_TAG: u16 = 0x0001;
/// 32-bit field value meaning "real value is in the ZIP64 extra field / EOCD64".
pub(super) const ZIP32_SENTINEL: u32 = 0xFFFF_FFFF;
/// 16-bit entry-count field value meaning "real count is in the EOCD64".
pub(super) const ZIP16_SENTINEL: u16 = 0xFFFF;
pub(super) const PHAR_WRITE_FD_BASE: usize = 0x5000_0000;
pub(super) const PHAR_WRITE_STREAM_LIMIT: usize = 32;
/// Largest allowed decompression expansion for one compressed archive entry.
pub(super) const MAX_PHAR_DECOMPRESSION_RATIO: usize = 1_024;
/// Absolute ceiling for one decoded PHAR, TAR, or ZIP entry.
pub(super) const MAX_PHAR_ENTRY_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;
/// Absolute ceiling for a whole gzip/bzip2 wrapped archive after decompression.
pub(super) const MAX_PHAR_ARCHIVE_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

pub(super) static WRITE_STREAMS: OnceLock<Mutex<Vec<Option<WriteStream>>>> = OnceLock::new();
thread_local! {
    /// Result buffer holding the bytes of the most recent extract/list/read call,
    /// whose pointer [`publish_result`] hands to the caller. Per-thread rather than
    /// process-global for a lifetime reason: refilling it reallocates, freeing the
    /// bytes any pointer handed out of it still points at, so a shared buffer would
    /// let one thread's call invalidate a pointer another thread is still reading
    /// (`elephc-pdo` shipped that bug and it reached CI as garbage bytes).
    pub(super) static EXTRACT_BUFFER: std::cell::RefCell<Vec<u8>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// Password used to read and write traditional-PKWARE (ZipCrypto) encrypted ZIP
    /// entries, set through [`elephc_phar_set_zip_password`]; `None` until provided.
    /// When set, zip phars are written with their entries encrypted. Thread-local:
    /// it is set and consumed on the same (single) runtime thread, which also keeps
    /// parallel unit tests from clobbering each other's password state.
    pub(super) static ZIP_PASSWORD: std::cell::RefCell<Option<Vec<u8>>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PharCompression {
    None,
    Gzip,
    Bzip2,
}

#[derive(Clone)]
pub(super) struct ArchiveEntry {
    pub(super) name: Vec<u8>,
    pub(super) payload: Vec<u8>,
    pub(super) compression: PharCompression,
    /// PHP-`serialize()`d per-file metadata blob (empty when the entry has none).
    /// Stored in the native manifest's per-entry metadata field, a tar
    /// `.phar/.metadata/<path>/.metadata.bin` side entry, or a zip central-directory
    /// file comment, depending on the archive family.
    pub(super) metadata: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(super) enum ArchiveFormat {
    NativePhar,
    Tar,
    Zip,
}

/// Internal entry name holding a tar/zip-based phar's executable stub.
pub(super) const PHAR_STUB_ENTRY: &[u8] = b".phar/stub.php";
/// Internal entry name holding a tar/zip-based phar's serialized global metadata.
pub(super) const PHAR_METADATA_ENTRY: &[u8] = b".phar/.metadata.bin";
/// Internal entry name holding a tar/zip-based phar's signature. Its payload is
/// `LE32(sig_flag) ++ LE32(sig_len) ++ signature`, and it is always the archive's
/// last entry so the signed range is everything that precedes it.
pub(super) const PHAR_SIGNATURE_ENTRY: &[u8] = b".phar/signature.bin";
/// Prefix of the tar side entry holding one file's serialized metadata. The full
/// name is `.phar/.metadata/<entry-path>/.metadata.bin` (matching php-src).
pub(super) const PHAR_FILE_METADATA_PREFIX: &[u8] = b".phar/.metadata/";
/// Suffix of the tar per-file metadata side entry (see [`PHAR_FILE_METADATA_PREFIX`]).
pub(super) const PHAR_FILE_METADATA_SUFFIX: &[u8] = b"/.metadata.bin";
/// Default native-PHAR stub emitted when no custom stub has been set.
pub(super) const PHAR_DEFAULT_STUB: &[u8] = b"<?php __HALT_COMPILER(); ?>\r\n";

/// A parsed archive plus its archive-level global metadata and stub.
///
/// `metadata` holds the PHP-`serialize()`d global metadata blob (empty when none);
/// `stub` holds the executable stub bytes (empty when none/default). Both are
/// preserved across read-modify-write cycles and re-emitted by [`build_archive`].
#[derive(Clone)]
pub(super) struct Archive {
    pub(super) entries: Vec<ArchiveEntry>,
    pub(super) format: ArchiveFormat,
    pub(super) metadata: Vec<u8>,
    pub(super) stub: Vec<u8>,
}

/// Returns true for the reserved `.phar/*` control entries that phars hide from
/// their public entry listing (stub, metadata, alias, signature, per-file metadata).
pub(super) fn is_phar_control_entry(name: &[u8]) -> bool {
    name.starts_with(b".phar/")
}

pub(super) enum WriteStreamTarget {
    Entry { archive: Vec<u8>, entry: Vec<u8> },
    Url(Vec<u8>),
}

pub(super) struct WriteStream {
    pub(super) target: WriteStreamTarget,
    pub(super) payload: Vec<u8>,
}
