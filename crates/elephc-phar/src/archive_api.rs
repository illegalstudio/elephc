//! Purpose:
//! Rust-facing PHAR extraction, listing, mutation, metadata, and stub operations.
//!
//! Called from:
//! - The crate facade, C ABI wrappers, and PHAR unit tests.
//!
//! Key details:
//! - Existing archive formats and per-entry compression are preserved on mutation.

use super::*;

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
    let path = std::path::Path::new(archive_path);
    let data = std::fs::read(path).ok()?;
    let openssl_expected = archive_public_key_path(path).exists();
    let public_key = read_archive_public_key(path);
    extract_archive_entry(&data, entry, public_key.as_ref(), openssl_expected)
}

/// Extracts `entry` from already-loaded archive bytes.
///
/// Container-family dispatch follows ZIP/TAR magic before falling back to native
/// PHAR. OpenSSL-signed archives are rejected because this byte-only API has no
/// filesystem path from which to load `<archive>.pubkey`; use [`extract_url_bytes`]
/// when signature authentication is required.
pub fn extract_entry_bytes(archive: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    extract_archive_entry(archive, entry, None, false)
}

/// Extracts one entry after authenticating and dispatching the archive family.
///
/// Each container parser authenticates and scans archive structure globally but
/// copies or decompresses only the requested payload.
fn extract_archive_entry(
    archive: &[u8],
    entry: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
    openssl_expected: bool,
) -> Option<Vec<u8>> {
    // Whole-archive gzip/bzip2 wrappers are decoded transparently before extraction.
    if archive.starts_with(b"\x1f\x8b") {
        return extract_archive_entry(
            &decompress_gzip_stream(archive)?,
            entry,
            public_key,
            openssl_expected,
        );
    }
    if archive.starts_with(b"BZh") {
        return extract_archive_entry(
            &decompress_bzip2_stream(archive)?,
            entry,
            public_key,
            openssl_expected,
        );
    }
    if archive.starts_with(b"PK\x03\x04") || archive.starts_with(b"PK\x05\x06") {
        parse_zip_entry_with_public_key(archive, entry, public_key, openssl_expected)
    } else if archive.get(257..262) == Some(b"ustar") {
        parse_tar_entry_with_public_key(archive, entry, public_key, openssl_expected)
    } else {
        parse_native_phar_entry_with_public_key(archive, entry, public_key, openssl_expected)
    }
}

/// Serializes every supported entry name from an archive on disk.
///
/// The output is a packed sequence of `u64 little-endian length` followed by
/// raw entry-name bytes. This keeps the C ABI simple while letting generated
/// code build a PHP string array without knowing the archive format.
pub fn entry_names_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let entries = parse_archive_path(std::path::Path::new(archive_path))?.entries;
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
    // A `.pubkey` sidecar expects an OpenSSL signature; this implicit rebuild
    // would replace it with SHA1 and leave the sidecar in place.
    if !implicit_rebuild_allowed(path) {
        return None;
    }
    let mut archive = if path.exists() {
        parse_archive_path(path)?
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
    if !implicit_rebuild_allowed(path) {
        return None;
    }
    let mut archive = parse_archive_path(path)?;
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
    if !implicit_rebuild_allowed(path) {
        return None;
    }
    let mut archive = parse_archive_path(path)?;
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
pub(super) fn get_metadata_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let path = std::str::from_utf8(archive_path).ok()?;
    Some(parse_archive_path(std::path::Path::new(path))?.metadata)
}

/// Reads an archive's stub bytes (empty when unset / default).
pub(super) fn get_stub_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let path = std::str::from_utf8(archive_path).ok()?;
    Some(parse_archive_path(std::path::Path::new(path))?.stub)
}

/// Sets an archive's global metadata, preserving all entries and the stub.
///
/// Creates the archive (format chosen by extension) when it does not yet exist.
pub(super) fn set_metadata_bytes(archive_path: &[u8], metadata: &[u8]) -> Option<()> {
    let path_str = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(path_str);
    if !implicit_rebuild_allowed(path) {
        return None;
    }
    let mut archive = read_or_new_archive(path)?;
    archive.metadata = metadata.to_vec();
    std::fs::write(path, build_archive_value(&archive)?).ok()?;
    Some(())
}

/// Sets an archive's stub, preserving all entries and global metadata.
///
/// The stub must contain `__HALT_COMPILER();` (matching PHP); creates the archive
/// (format chosen by extension) when it does not yet exist.
pub(super) fn set_stub_bytes(archive_path: &[u8], stub: &[u8]) -> Option<()> {
    if find_subslice(stub, b"__HALT_COMPILER();").is_none() {
        return None;
    }
    let path_str = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(path_str);
    if !implicit_rebuild_allowed(path) {
        return None;
    }
    let mut archive = read_or_new_archive(path)?;
    archive.stub = stub.to_vec();
    std::fs::write(path, build_archive_value(&archive)?).ok()?;
    Some(())
}

/// Parses an existing archive, or builds an empty one whose format follows the path.
pub(super) fn read_or_new_archive(path: &std::path::Path) -> Option<Archive> {
    if path.exists() {
        parse_archive_path(path)
    } else {
        Some(Archive {
            entries: Vec::new(),
            format: format_for_new_archive_path(path),
            metadata: Vec::new(),
            stub: Vec::new(),
        })
    }
}
