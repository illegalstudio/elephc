//! Purpose:
//! Numeric constants shared by the ZIP engine, matching PHP's `ZipArchive::*` values.
//!
//! Called from:
//! - Metadata parsing, archive mutation, and the C ABI's supported-method queries.
//!
//! Key details:
//! - Values are the libzip 1.11.4 constants PHP 8.5 exposes.

/// Stored (no compression).
pub(crate) const CM_STORE: i64 = 0;
/// Default compression (libzip chooses deflate).
pub(crate) const CM_DEFAULT: i64 = -1;
/// Deflate compression.
pub(crate) const CM_DEFLATE: i64 = 8;
/// Bzip2 compression.
pub(crate) const CM_BZIP2: i64 = 12;
/// LZMA compression.
pub(crate) const CM_LZMA: i64 = 14;
/// XZ compression.
pub(crate) const CM_XZ: i64 = 95;
/// PPMd compression.
pub(crate) const CM_PPMD: i64 = 98;

/// No encryption.
pub(crate) const EM_NONE: i64 = 0;
/// Traditional PKWARE encryption.
pub(crate) const EM_TRAD_PKWARE: i64 = 1;
/// AES-128 encryption.
pub(crate) const EM_AES_128: i64 = 257;
/// AES-192 encryption.
pub(crate) const EM_AES_192: i64 = 258;
/// AES-256 encryption.
pub(crate) const EM_AES_256: i64 = 259;
/// Unknown encryption method.
pub(crate) const EM_UNKNOWN: i64 = 65535;

/// `ZipArchive::CREATE`.
pub(crate) const CREATE: i64 = 1;
/// `ZipArchive::EXCL`.
pub(crate) const EXCL: i64 = 2;
/// `ZipArchive::CHECKCONS`.
pub(crate) const CHECKCONS: i64 = 4;
/// `ZipArchive::OVERWRITE`.
pub(crate) const OVERWRITE: i64 = 8;
/// `ZipArchive::RDONLY`.
pub(crate) const RDONLY: i64 = 16;

/// `ZipArchive::FL_NOCASE`.
pub(crate) const FL_NOCASE: i64 = 1;
/// `ZipArchive::FL_NODIR`.
pub(crate) const FL_NODIR: i64 = 2;
/// `ZipArchive::FL_COMPRESSED`.
pub(crate) const FL_COMPRESSED: i64 = 4;
/// `ZipArchive::FL_UNCHANGED`.
pub(crate) const FL_UNCHANGED: i64 = 8;
/// `ZipArchive::FL_ENCRYPTED`.
pub(crate) const FL_ENCRYPTED: i64 = 32;
/// `ZipArchive::FL_ENC_RAW`.
pub(crate) const FL_ENC_RAW: i64 = 64;
/// `ZipArchive::FL_ENC_UTF_8`.
pub(crate) const FL_ENC_UTF_8: i64 = 2048;
/// `ZipArchive::FL_ENC_CP437`.
pub(crate) const FL_ENC_CP437: i64 = 4096;
/// `ZipArchive::FL_OVERWRITE`.
pub(crate) const FL_OVERWRITE: i64 = 8192;

/// `ZipArchive::AFL_RDONLY`.
pub(crate) const AFL_RDONLY: i64 = 2;
/// `ZipArchive::AFL_IS_TORRENTZIP`.
pub(crate) const AFL_IS_TORRENTZIP: i64 = 4;
/// `ZipArchive::AFL_WANT_TORRENTZIP`.
pub(crate) const AFL_WANT_TORRENTZIP: i64 = 8;
/// `ZipArchive::AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE`.
pub(crate) const AFL_KEEP_EMPTY: i64 = 16;

/// `ZipArchive::LENGTH_TO_END`.
pub(crate) const LENGTH_TO_END: i64 = 0;
/// `ZipArchive::LENGTH_UNCHECKED`.
pub(crate) const LENGTH_UNCHECKED: i64 = -2;
