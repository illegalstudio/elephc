//! Purpose:
//! Native PHAR manifest parsing, entry mutation, metadata, and archive emission.
//!
//! Called from:
//! - Shared archive dispatch and Rust-facing PHAR operations.
//!
//! Key details:
//! - Per-entry compression and metadata are preserved across rebuilds.

use super::*;

/// Parses a native PHAR archive into entries plus its global metadata and stub.
///
/// The stub is the byte prefix up to and including the `__HALT_COMPILER();` marker
/// (and any trailing ` ?>\r\n`); the global metadata is the manifest's metadata field.
#[cfg(test)]
pub(super) fn parse_native_phar_archive(data: &[u8]) -> Option<Archive> {
    parse_native_phar_archive_with_public_key(data, None, false, false)
}

/// Locates the native manifest just past the `__HALT_COMPILER();` stub marker.
///
/// The parser and the signature-only probe share this offset so signing and
/// verification never disagree about where the manifest begins.
pub(super) fn native_manifest_start(data: &[u8]) -> Option<usize> {
    let halt = b"__HALT_COMPILER();";
    let halt_idx = find_subslice(data, halt)?;
    let mut p = halt_idx + halt.len();
    for &ch in &[b' ', b'?', b'>', b'\r', b'\n'] {
        if data.get(p) == Some(&ch) {
            p += 1;
        }
    }
    Some(p)
}

/// Parses a native PHAR and authenticates an OpenSSL trailer with `public_key`.
///
/// `openssl_expected` reports the adjacent `<archive>.pubkey` sidecar; when set,
/// a keyless hash signature is rejected instead of trusted. `require_hash`
/// mirrors `phar.require_hash`: when true, an unsigned archive is rejected.
pub(super) fn parse_native_phar_archive_with_public_key(
    data: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
    openssl_expected: bool,
    require_hash: bool,
) -> Option<Archive> {
    let manifest_start = native_manifest_start(data)?;
    let stub = data.get(..manifest_start)?.to_vec();
    let manifest_len = le32(data, manifest_start)? as usize;
    let data_section = manifest_start.checked_add(4)?.checked_add(manifest_len)?;
    data.get(..data_section)?;

    let num_files = le32(data, manifest_start + 4)?;
    let global_flags = le32(data, manifest_start + 10)?;
    verify_native_phar_signature(
        data,
        global_flags & PHAR_HDR_SIGNATURE != 0,
        public_key,
        openssl_expected,
        require_hash,
    )?;
    let mut q = manifest_start + 8 + 2 + 4;
    let alias_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(alias_len)?;
    let meta_len = le32(data, q)? as usize;
    q = q.checked_add(4)?;
    let metadata = data.get(q..q.checked_add(meta_len)?)?.to_vec();
    q = q.checked_add(meta_len)?;

    let manifest_remaining = data_section.checked_sub(q)?;
    // Every entry needs at least a name length and six 32-bit metadata words.
    if usize::try_from(num_files).ok()? > manifest_remaining / 28 {
        return None;
    }
    let mut data_offset = 0usize;
    let mut entries = Vec::new();
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
        signature_flag: if global_flags & PHAR_HDR_SIGNATURE != 0 {
            Some(le32(data, data.len().checked_sub(8)?)?)
        } else {
            None
        },
    })
}

/// Authenticates and scans a native PHAR while decoding only `entry`.
///
/// Every manifest record and stored-data extent is validated before the selected
/// payload is decoded. Unrelated compressed entries therefore cannot consume the
/// requested entry's decompression budget or force a second payload allocation.
pub(super) fn parse_native_phar_entry_with_public_key(
    data: &[u8],
    entry: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
    openssl_expected: bool,
    require_hash: bool,
) -> Option<Vec<u8>> {
    let halt = b"__HALT_COMPILER();";
    let halt_idx = find_subslice(data, halt)?;
    let mut p = halt_idx.checked_add(halt.len())?;
    for &ch in &[b' ', b'?', b'>', b'\r', b'\n'] {
        if data.get(p) == Some(&ch) {
            p = p.checked_add(1)?;
        }
    }

    let manifest_start = p;
    let manifest_len = le32(data, manifest_start)? as usize;
    let data_section = manifest_start.checked_add(4)?.checked_add(manifest_len)?;
    data.get(..data_section)?;
    let num_files = le32(data, manifest_start.checked_add(4)?)?;
    let global_flags = le32(data, manifest_start.checked_add(10)?)?;
    verify_native_phar_signature(
        data,
        global_flags & PHAR_HDR_SIGNATURE != 0,
        public_key,
        openssl_expected,
        require_hash,
    )?;
    let mut q = manifest_start.checked_add(8 + 2 + 4)?;
    let alias_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(alias_len)?;
    let metadata_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(metadata_len)?;

    let manifest_remaining = data_section.checked_sub(q)?;
    if usize::try_from(num_files).ok()? > manifest_remaining / 28 {
        return None;
    }
    let mut data_offset = 0usize;
    let mut selected: Option<(&[u8], u32, usize)> = None;
    for _ in 0..num_files {
        let name_len = le32(data, q)? as usize;
        q = q.checked_add(4)?;
        let name = data.get(q..q.checked_add(name_len)?)?;
        q = q.checked_add(name_len)?;
        let uncompressed = le32(data, q)? as usize;
        q = q.checked_add(8)?; // uncompressed size and timestamp
        let compressed = le32(data, q)? as usize;
        q = q.checked_add(8)?; // compressed size and CRC32
        let flags = le32(data, q)?;
        q = q.checked_add(4)?;
        let entry_metadata_len = le32(data, q)? as usize;
        q = q.checked_add(4)?.checked_add(entry_metadata_len)?;

        let start = data_section.checked_add(data_offset)?;
        let stored = data.get(start..start.checked_add(compressed)?)?;
        if selected.is_none() && name == entry {
            selected = Some((stored, flags, uncompressed));
        }
        data_offset = data_offset.checked_add(compressed)?;
    }
    let (stored, flags, uncompressed) = selected?;
    decode_phar_payload(stored, flags, uncompressed)
}

/// Extracts the PHAR compression mode from per-entry flags.
pub(super) fn phar_compression_from_flags(flags: u32) -> PharCompression {
    if flags & PHAR_FLAG_GZIP != 0 {
        PharCompression::Gzip
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        PharCompression::Bzip2
    } else {
        PharCompression::None
    }
}

/// Decodes a native PHAR entry payload according to its per-entry flags.
/// Rejects implausible declared expansion and stops after one byte beyond the
/// claimed output size so a forged header cannot drive unbounded decompression.
pub(super) fn decode_phar_payload(stored: &[u8], flags: u32, uncompressed: usize) -> Option<Vec<u8>> {
    if uncompressed > MAX_PHAR_ENTRY_DECOMPRESSED_BYTES {
        return None;
    }
    if flags & PHAR_FLAG_GZIP != 0 {
        if uncompressed > stored.len().checked_mul(MAX_PHAR_DECOMPRESSION_RATIO)? {
            return None;
        }
        let mut out = Vec::new();
        let decoder = flate2::read::DeflateDecoder::new(stored);
        decoder
            .take(u64::try_from(uncompressed.checked_add(1)?).ok()?)
            .read_to_end(&mut out)
            .ok()?;
        (out.len() == uncompressed).then_some(out)
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        if uncompressed > stored.len().checked_mul(MAX_PHAR_DECOMPRESSION_RATIO)? {
            return None;
        }
        let mut out = Vec::new();
        let decoder = bzip2_rs::DecoderReader::new(stored);
        decoder
            .take(u64::try_from(uncompressed.checked_add(1)?).ok()?)
            .read_to_end(&mut out)
            .ok()?;
        (out.len() == uncompressed).then_some(out)
    } else {
        (stored.len() == uncompressed).then(|| stored.to_vec())
    }
}

/// Inserts `payload` under `entry_name`, preserving compression for replacements.
pub(super) fn upsert_entry(entries: &mut Vec<ArchiveEntry>, entry_name: &[u8], payload: &[u8]) {
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
pub(super) fn get_file_metadata_bytes(archive_path: &[u8], entry_name: &[u8]) -> Option<Vec<u8>> {
    let path = std::path::Path::new(std::str::from_utf8(archive_path).ok()?);
    let archive = parse_archive_path(path)?;
    let entry = archive.entries.iter().find(|e| e.name == entry_name)?;
    Some(entry.metadata.clone())
}

/// Sets (or clears, when `metadata` is empty) the per-file metadata for
/// `entry_name` and rewrites the archive. Fails if the entry does not exist.
pub(super) fn set_file_metadata_bytes(
    archive_path: &[u8],
    entry_name: &[u8],
    metadata: &[u8],
) -> Option<()> {
    let path = std::path::Path::new(std::str::from_utf8(archive_path).ok()?);
    if !implicit_rebuild_allowed(path) {
        return None;
    }
    let mut archive = parse_archive_path(path)?;
    let entry = archive.entries.iter_mut().find(|e| e.name == entry_name)?;
    entry.metadata.clear();
    entry.metadata.extend_from_slice(metadata);
    let rebuilt = build_archive_value(&archive)?;
    std::fs::write(path, rebuilt).ok()
}

/// Reads per-file metadata addressed by a `phar://archive/entry` URL, splitting it
/// into archive path and entry name before delegating to [`get_file_metadata_bytes`].
pub(super) fn get_file_metadata_url(url: &[u8]) -> Option<Vec<u8>> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry) = split_archive_entry(rest)?;
    get_file_metadata_bytes(archive_path, entry)
}

/// Writes per-file metadata addressed by a `phar://archive/entry` URL, splitting it
/// into archive path and entry name before delegating to [`set_file_metadata_bytes`].
pub(super) fn set_file_metadata_url(url: &[u8], metadata: &[u8]) -> Option<()> {
    let rest = url.strip_prefix(b"phar://")?;
    let (archive_path, entry) = split_archive_entry(rest)?;
    set_file_metadata_bytes(archive_path, entry, metadata)
}

/// Removes an archive entry and reports failure when no matching entry exists.
pub(super) fn remove_entry(entries: &mut Vec<ArchiveEntry>, entry_name: &[u8]) -> Option<()> {
    let index = entries.iter().position(|entry| entry.name == entry_name)?;
    entries.remove(index);
    Some(())
}

/// Builds a native PHAR archive from decoded entries, signed with
/// `signature_flag` when present.
pub(super) fn build_native_phar_archive(
    entries: &[ArchiveEntry],
    metadata: &[u8],
    stub: &[u8],
    signature_flag: Option<u32>,
) -> Option<Vec<u8>> {
    let mut manifest = Vec::new();
    let mut stored_entries = Vec::with_capacity(entries.len());
    manifest.extend_from_slice(&u32::try_from(entries.len()).ok()?.to_le_bytes());
    manifest.extend_from_slice(&[0x11, 0x00]);
    let global_flags = if signature_flag.is_some() {
        PHAR_HDR_SIGNATURE
    } else {
        0
    };
    manifest.extend_from_slice(&global_flags.to_le_bytes());
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
    if let Some(flag) = signature_flag {
        let digest = compute_signature(flag, None, &out)?;
        out.extend_from_slice(&digest);
        out.extend_from_slice(&flag.to_le_bytes());
        out.extend_from_slice(b"GBMB");
    }
    Some(out)
}

/// Encodes a native PHAR payload according to its preserved compression mode.
pub(super) fn encode_phar_payload(payload: &[u8], compression: PharCompression) -> Option<Vec<u8>> {
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
pub(super) fn phar_compression_flag(compression: PharCompression) -> u32 {
    match compression {
        PharCompression::None => 0,
        PharCompression::Gzip => PHAR_FLAG_GZIP,
        PharCompression::Bzip2 => PHAR_FLAG_BZIP2,
    }
}

/// Converts PHP's PHAR compression constants into bridge compression modes.
pub(super) fn compression_from_php_constant(value: usize) -> Option<PharCompression> {
    match value {
        0 => Some(PharCompression::None),
        4_096 => Some(PharCompression::Gzip),
        8_192 => Some(PharCompression::Bzip2),
        _ => None,
    }
}
