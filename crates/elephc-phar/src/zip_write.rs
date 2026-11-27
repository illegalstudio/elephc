//! Purpose:
//! ZIP and ZIP64 PHAR serialization, including deflate and ZipCrypto payloads.
//!
//! Called from:
//! - Shared archive rebuild and ZIP signature generation.
//!
//! Key details:
//! - ZIP64 sentinels and central-directory offsets remain internally consistent.

use super::*;

/// Builds a ZIP archive with stored or deflated entries and central-directory records.
pub(super) fn build_zip_archive(entries: &[ArchiveEntry], metadata: &[u8], stub: &[u8]) -> Option<Vec<u8>> {
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
pub(super) fn write_zip_body(
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
pub(super) fn finalize_zip(out: &mut Vec<u8>, central: &[u8], count: usize, metadata: &[u8]) -> Option<()> {
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
pub(super) fn sign_zip_archive(
    entries: &[ArchiveEntry],
    metadata: &[u8],
    stub: &[u8],
    flag: u32,
    key: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut count = write_zip_body(&mut out, &mut central, entries, stub)?;
    // Signed range: local entries ++ central records ++ comment, signature excluded.
    let mut signed = out.clone();
    signed.extend_from_slice(&central);
    signed.extend_from_slice(metadata);
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
    finalize_zip(&mut out, &central, count, metadata)?;
    Some(out)
}

/// Writes one ZIP entry: its local file header + stored payload into `out`, and the
/// matching central-directory record into `central`. When `encrypt` is set and a zip
/// password is configured, the stored payload is ZipCrypto-encrypted and the
/// general-purpose "encrypted" flag is set in both headers.
pub(super) fn write_zip_entry(
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
pub(super) fn zip64_local_extra(uncompressed: u64, compressed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&ZIP64_EXTRA_TAG.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(&uncompressed.to_le_bytes());
    out.extend_from_slice(&compressed.to_le_bytes());
    out
}

/// Builds a ZIP64 central-directory extra field holding only the overflowed
/// fields, in APPNOTE order: uncompressed size, compressed size, header offset.
pub(super) fn zip64_central_extra(
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
pub(super) fn encode_zip_payload(payload: &[u8], compression: PharCompression) -> Option<(u16, Vec<u8>)> {
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
