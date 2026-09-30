//! Purpose:
//! Rust-facing PHAR extraction, listing, mutation, metadata, and stub operations.
//!
//! Called from:
//! - The crate facade, C ABI wrappers, and PHAR unit tests.
//!
//! Key details:
//! - Existing archive formats and per-entry compression are preserved on mutation.

use super::*;

/// Extracts a `phar://archive/entry` or `zip://archive#entry` URL into bytes.
///
/// For `phar://` the archive portion is found by scanning slash-delimited
/// prefixes until one names an existing file. This matches PHP's
/// archive-boundary behavior while also supporting `.phar`, `.tar`, and `.zip`
/// suffixes without hardcoding an extension list.
///
/// For `zip://` php's `ext/zip` wrapper uses a completely different URL shape —
/// a single `#` separates the archive from the entry — and reads the entry as a
/// plain ZIP member with no phar semantics. Both schemes share this entry point
/// because the generated runtime reaches the bridge through one function
/// pointer, and the scheme in the URL already says which shape to parse.
pub fn extract_url_bytes(url: &[u8]) -> Option<Vec<u8>> {
    if url.starts_with(b"zip://") {
        return zip_extract_url_bytes(url);
    }
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry) = split_archive_entry(rest)?;
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(archive_path);
    let data = std::fs::read(path).ok()?;
    let public_key = read_archive_public_key(path);
    extract_archive_entry(&data, entry, public_key.as_ref())
}

/// Splits a `zip://<archive>#<entry>` URL at its FIRST `#`.
///
/// Measured on `php -n` 8.5.6: an archive holding an entry literally named
/// `a#b.txt` is read by `zip://h.zip#a#b.txt`, so the separator is the first `#`
/// and every later one belongs to the entry name. An empty archive path or an
/// empty entry name has no member to name, and php fails both.
pub(super) fn split_zip_url(url: &[u8]) -> Option<(&[u8], &[u8])> {
    let rest = url.strip_prefix(b"zip://")?;
    let hash = rest.iter().position(|&byte| byte == b'#')?;
    let (archive_path, entry) = rest.split_at(hash);
    let entry = entry.get(1..)?;
    (!archive_path.is_empty() && !entry.is_empty()).then_some((archive_path, entry))
}

/// Serializes every ZIP entry's `ZipArchive::statIndex()` fields for `archive_path`.
///
/// Returns `None` when the file cannot be read or is no ZIP at all — the two cases
/// php's `ZipArchive::open()` answers with `ER_NOENT` and `ER_NOZIP`. See
/// [`zip_stat_records`] for the wire shape.
pub fn zip_stat_entries_bytes(archive_path: &[u8]) -> Option<Vec<u8>> {
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let archive = std::fs::read(archive_path).ok()?;
    zip_stat_records(&archive)
}

/// Reads one entry out of the ZIP archive a `zip://archive#entry` URL names.
///
/// Returns `None` for a missing archive, a missing entry, a `#`-less URL, and an
/// encrypted entry with no password — every one of which php reports as the same
/// failed open.
pub fn zip_extract_url_bytes(url: &[u8]) -> Option<Vec<u8>> {
    let (archive_path, entry) = split_zip_url(url)?;
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let archive = std::fs::read(archive_path).ok()?;
    zip_entry_payload(&archive, entry)
}

/// Extracts `entry` from already-loaded archive bytes.
///
/// Container-family dispatch follows ZIP/TAR magic before falling back to native
/// PHAR. OpenSSL-signed archives are rejected because this byte-only API has no
/// filesystem path from which to load `<archive>.pubkey`; use [`extract_url_bytes`]
/// when signature authentication is required.
pub fn extract_entry_bytes(archive: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    extract_archive_entry(archive, entry, None)
}

/// Extracts one entry after authenticating and dispatching the archive family.
///
/// Each container parser authenticates and scans archive structure globally but
/// copies or decompresses only the requested payload.
fn extract_archive_entry(
    archive: &[u8],
    entry: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
) -> Option<Vec<u8>> {
    // Whole-archive gzip/bzip2 wrappers are decoded transparently before extraction.
    if archive.starts_with(b"\x1f\x8b") {
        return extract_archive_entry(&decompress_gzip_stream(archive)?, entry, public_key);
    }
    if archive.starts_with(b"BZh") {
        return extract_archive_entry(&decompress_bzip2_stream(archive)?, entry, public_key);
    }
    if archive.starts_with(b"PK\x03\x04") || archive.starts_with(b"PK\x05\x06") {
        parse_zip_entry_with_public_key(archive, entry, public_key)
    } else if archive.get(257..262) == Some(b"ustar") {
        parse_tar_entry_with_public_key(archive, entry, public_key)
    } else {
        parse_native_phar_entry_with_public_key(archive, entry, public_key)
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
