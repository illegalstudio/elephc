//! Purpose:
//! CRC-32, deflate sizing, and compression-method support for archive mutations.
//!
//! Called from:
//! - `crate::model::Entry` refresh paths and `crate::write` when re-emitting entries.
//!
//! Key details:
//! - Stat-time sizes use the same flate2 encoder the writer uses, so `comp_size` is stable.

use std::io::Write;

use crate::consts::*;

/// Computes the CRC-32 of a byte slice.
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// Returns the deflate-compressed representation of a payload.
pub(crate) fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = encoder.write_all(data);
    encoder.finish().unwrap_or_default()
}

/// Returns the compression method's stored byte count for stat reporting.
pub(crate) fn stored_size(method: i64, data: &[u8]) -> u64 {
    match method {
        CM_STORE => data.len() as u64,
        CM_DEFLATE | CM_DEFAULT => deflate(data).len() as u64,
        _ => data.len() as u64,
    }
}

/// Computes the compressed size and CRC used when a memory entry is created.
pub(crate) fn deflate_stats(data: &[u8]) -> (u64, u32) {
    (deflate(data).len() as u64, crc32(data))
}

/// Returns the compressed payload for `FL_COMPRESSED` reads of memory entries.
pub(crate) fn compress(method: i64, data: &[u8]) -> Vec<u8> {
    match method {
        CM_STORE => data.to_vec(),
        _ => deflate(data),
    }
}

/// Returns whether this engine can write entries with `method`.
pub(crate) fn writable_method(method: i64) -> bool {
    matches!(method, CM_DEFAULT | CM_STORE | CM_DEFLATE | CM_BZIP2)
}

/// Maps a libzip method id to the zip writer's enum.
pub(crate) fn engine_method(method: i64) -> Option<zip::CompressionMethod> {
    match method {
        CM_DEFAULT | CM_DEFLATE => Some(zip::CompressionMethod::Deflated),
        CM_STORE => Some(zip::CompressionMethod::Stored),
        CM_BZIP2 => Some(zip::CompressionMethod::Bzip2),
        _ => None,
    }
}

/// Reports whether a compression method can be both read and written by this engine.
pub(crate) fn supported_compression(method: i64, encode: bool) -> bool {
    if encode {
        return writable_method(method);
    }
    matches!(
        method,
        CM_DEFAULT | CM_STORE | CM_DEFLATE | CM_BZIP2 | CM_LZMA | CM_XZ | CM_PPMD
    )
}

/// Reports whether an encryption method can be read and written by this engine.
pub(crate) fn supported_encryption(method: i64, _encode: bool) -> bool {
    match method {
        EM_NONE => true,
        EM_TRAD_PKWARE | EM_AES_128 | EM_AES_192 | EM_AES_256 => true,
        _ => false,
    }
}
