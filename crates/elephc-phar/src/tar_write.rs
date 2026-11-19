//! Purpose:
//! Tar-based PHAR serialization and ustar header construction.
//!
//! Called from:
//! - Shared archive rebuild and tar signature generation.
//!
//! Key details:
//! - Reserved PHAR control entries are emitted without leaking into public listings.

use super::*;

/// Builds a POSIX ustar archive with stored regular-file entries.
pub(super) fn build_tar_archive(entries: &[ArchiveEntry], metadata: &[u8], stub: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    write_tar_body(&mut out, entries, metadata, stub)?;
    out.extend_from_slice(&[0u8; 1024]);
    Some(out)
}

/// Writes a tar phar's data records (stub, global metadata, entries, and per-file
/// metadata side entries) into `out`, without the trailing zero blocks. The bytes
/// it produces are exactly the range a tar phar signature is computed over.
pub(super) fn write_tar_body(
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
pub(super) fn sign_tar_archive(
    entries: &[ArchiveEntry],
    metadata: &[u8],
    stub: &[u8],
    flag: u32,
    key: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    write_tar_body(&mut out, entries, metadata, stub)?;
    let sig = compute_signature(flag, key, &out)?;
    write_tar_entry(&mut out, PHAR_SIGNATURE_ENTRY, &signature_bin_payload(flag, &sig)?)?;
    out.extend_from_slice(&[0u8; 1024]);
    Some(out)
}

/// Builds the tar side-entry path that holds one file's serialized metadata.
pub(super) fn tar_file_metadata_name(entry_name: &[u8]) -> Vec<u8> {
    let mut name = Vec::with_capacity(
        PHAR_FILE_METADATA_PREFIX.len() + entry_name.len() + PHAR_FILE_METADATA_SUFFIX.len(),
    );
    name.extend_from_slice(PHAR_FILE_METADATA_PREFIX);
    name.extend_from_slice(entry_name);
    name.extend_from_slice(PHAR_FILE_METADATA_SUFFIX);
    name
}

/// Writes one uncompressed POSIX ustar entry (512-byte header + padded payload).
pub(super) fn write_tar_entry(out: &mut Vec<u8>, entry_name: &[u8], payload: &[u8]) -> Option<()> {
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
pub(super) fn split_tar_name(name: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
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
