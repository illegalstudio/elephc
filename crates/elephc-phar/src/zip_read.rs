//! Purpose:
//! ZIP and ZIP64 central-directory parsing and entry payload decoding.
//!
//! Called from:
//! - Shared archive dispatch and ZIP extraction operations.
//!
//! Key details:
//! - Streaming descriptors, ZIP64 extras, deflate, and ZipCrypto are decoded here.

use super::*;

/// Authenticates a ZIP PHAR and decodes only the requested non-control entry.
pub(super) fn parse_zip_entry_with_public_key(
    data: &[u8],
    entry: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
) -> Option<Vec<u8>> {
    verify_zip_phar_signature(data, public_key)?;
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
        let entry_comment_len = le16(data, p + 32)? as usize;
        let mut local_offset = le32(data, p + 42)? as usize;
        let name_start = p.checked_add(46)?;
        let name = data.get(name_start..name_start.checked_add(name_len)?)?;
        apply_zip64_central_extra(
            data,
            name_start.checked_add(name_len)?,
            extra_len,
            &mut uncompressed_size,
            &mut compressed_size,
            &mut local_offset,
        )?;
        let central_end = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(entry_comment_len)?;
        if name == entry && !is_phar_control_entry(name) {
            let (encrypted, check_byte) = zip_entry_crypto(data, p)?;
            let crc = le32(data, p + 16)?;
            return decode_zip_local_entry(
                data,
                local_offset,
                method,
                compressed_size,
                uncompressed_size,
                encrypted,
                check_byte,
                crc,
            );
        }
        p = central_end;
    }
    None
}

/// Parses a zip-based phar into entries plus its global metadata and stub.
///
/// Global metadata is read from the EOCD archive comment; the reserved
/// `.phar/stub.php` entry becomes the stub and other `.phar/*` control entries are
/// hidden from the entry listing.
#[cfg(test)]
pub(super) fn parse_zip_archive(data: &[u8]) -> Option<Archive> {
    parse_zip_archive_with_public_key(data, None)
}

/// Parses a zip-based PHAR and authenticates an OpenSSL signature with `public_key`.
pub(super) fn parse_zip_archive_with_public_key(
    data: &[u8],
    public_key: Option<&rsa::RsaPublicKey>,
) -> Option<Archive> {
    verify_zip_phar_signature(data, public_key)?;
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
        let crc = le32(data, p + 16)?;
        let payload = decode_zip_local_entry(
            data,
            local_offset,
            method,
            compressed_size,
            uncompressed_size,
            encrypted,
            check_byte,
            crc,
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
pub(super) fn zip_compression_from_method(method: u16) -> Option<PharCompression> {
    match method {
        ZIP_METHOD_STORE => Some(PharCompression::None),
        ZIP_METHOD_DEFLATE => Some(PharCompression::Gzip),
        _ => None,
    }
}

/// Finds the ZIP end-of-central-directory record.
pub(super) fn find_zip_eocd(data: &[u8]) -> Option<usize> {
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
pub(super) fn zip_eocd_info(data: &[u8]) -> Option<(usize, usize)> {
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
pub(super) fn read_zip64_eocd(data: &[u8], eocd: usize) -> Option<(usize, usize)> {
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
pub(super) fn apply_zip64_central_extra(
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
pub(super) fn zip_entry_crypto(data: &[u8], central_off: usize) -> Option<(bool, u8)> {
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
/// return `None`. The decoded bytes must also match the central-directory CRC,
/// because ZipCrypto's one-byte header check can collide for an incorrect password.
pub(super) fn decode_zip_local_entry(
    data: &[u8],
    local_offset: usize,
    method: u16,
    compressed_size: usize,
    uncompressed_size: usize,
    encrypted: bool,
    check_byte: u8,
    expected_crc: u32,
) -> Option<Vec<u8>> {
    if uncompressed_size > MAX_PHAR_ENTRY_DECOMPRESSED_BYTES {
        return None;
    }
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
    let decoded = match method {
        ZIP_METHOD_STORE => (body.len() == uncompressed_size).then(|| body.to_vec()),
        ZIP_METHOD_DEFLATE => {
            if uncompressed_size > body.len().checked_mul(MAX_PHAR_DECOMPRESSION_RATIO)? {
                return None;
            }
            let mut out = Vec::new();
            let decoder = flate2::read::DeflateDecoder::new(body);
            decoder
                .take(u64::try_from(uncompressed_size.checked_add(1)?).ok()?)
                .read_to_end(&mut out)
                .ok()?;
            (out.len() == uncompressed_size).then_some(out)
        }
        _ => None,
    }?;
    (crc32(&decoded) == expected_crc).then_some(decoded)
}
