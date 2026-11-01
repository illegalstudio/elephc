//! Purpose:
//! Parses ZIP central-directory metadata, ZIP64 extras, AES extras, and Unix timestamps.
//!
//! Called from:
//! - `crate::model::Archive::open_bytes` and the archive writer's raw-copy path.
//!
//! Key details:
//! - Names stay as the stored bytes; `decode_name` applies libzip's CP437/UTF-8 guess.
//! - Malformed archives answer `ZipError::Code` values rather than panicking.

use crate::error::ZipError;

/// One central-directory entry exactly as stored in the archive.
#[derive(Clone, Debug, Default)]
pub(crate) struct RawEntry {
    /// Stored file-name bytes.
    pub(crate) name_raw: Vec<u8>,
    /// General-purpose bit flags.
    pub(crate) flags: u16,
    /// Stored compression method id.
    pub(crate) method: u16,
    /// CRC-32 of the uncompressed bytes.
    pub(crate) crc: u32,
    /// Compressed payload size.
    pub(crate) comp_size: u64,
    /// Uncompressed payload size.
    pub(crate) size: u64,
    /// Per-entry comment bytes.
    pub(crate) comment: Vec<u8>,
    /// External attributes from the central record.
    pub(crate) external_attrs: u32,
    /// Version-made-by host system byte.
    pub(crate) system: u8,
    /// Offset of the local file header.
    pub(crate) local_offset: u64,
    /// Unix mtime from the extended-timestamp extra, or the DOS time converted to UTC.
    pub(crate) mtime: i64,
    /// libzip encryption method (`EM_*`).
    pub(crate) encryption: i64,
    /// AES actual compression method when the entry is AES-encrypted.
    pub(crate) aes_method: Option<u16>,
}

/// Parsed archive directory plus its comment bytes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Directory {
    /// Entries in central-directory order.
    pub(crate) entries: Vec<RawEntry>,
    /// Archive comment bytes.
    pub(crate) comment: Vec<u8>,
    /// Whether an EOCD comment carries the torrentzip marker.
    pub(crate) torrentzip: bool,
}

/// Reads a little-endian `u16` at `offset`, rejecting offsets whose end overflows.
fn le16(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

/// Reads a little-endian `u32` at `offset`, rejecting offsets whose end overflows.
fn le32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

/// Reads a little-endian `u64` at `offset`, rejecting offsets whose end overflows.
fn le64(data: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

/// Finds the end-of-central-directory record, tolerating a bounded trailing comment.
fn find_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let start = data.len().saturating_sub(65_557);
    (start..=data.len() - 22)
        .rev()
        .find(|&offset| le32(data, offset) == Some(0x0605_4b50))
}

/// Converts a DOS date/time pair into a Unix timestamp in UTC.
fn dos_to_unix(date: u16, time: u16) -> i64 {
    let year = 1980 + i64::from((date >> 9) & 0x7f);
    let month = i64::from((date >> 5) & 0x0f).clamp(1, 12);
    let day = i64::from(date & 0x1f).clamp(1, 31);
    let hour = i64::from((time >> 11) & 0x1f);
    let minute = i64::from((time >> 5) & 0x3f);
    let second = i64::from(time & 0x1f) * 2;
    crate::timeutil::civil_to_unix(year, month, day, hour, minute, second)
}

/// Parses the extra-field stream for ZIP64 sizes, AES parameters, and Unix mtime.
fn parse_extra(
    extra: &[u8],
    entry: &mut RawEntry,
    needs_zip64_size: bool,
    needs_zip64_offset: bool,
) {
    let mut position = 0usize;
    while position + 4 <= extra.len() {
        let Some(tag) = le16(extra, position) else { break };
        let Some(length) = le16(extra, position + 2).map(usize::from) else { break };
        let body_start = position + 4;
        let Some(body) = extra.get(body_start..body_start + length) else { break };
        match tag {
            0x0001 => {
                let mut cursor = 0usize;
                if needs_zip64_size {
                    if let Some(value) = le64(body, cursor) { entry.size = value; cursor += 8; }
                    if let Some(value) = le64(body, cursor) { entry.comp_size = value; cursor += 8; }
                }
                if needs_zip64_offset {
                    if let Some(value) = le64(body, cursor) { entry.local_offset = value; }
                }
            }
            0x9901 if body.len() >= 7 => {
                let strength = body[4];
                entry.aes_method = le16(body, 5);
                entry.encryption = match strength {
                    1 => crate::consts::EM_AES_128,
                    2 => crate::consts::EM_AES_192,
                    3 => crate::consts::EM_AES_256,
                    _ => crate::consts::EM_UNKNOWN,
                };
            }
            0x5455 if !body.is_empty() => {
                let flags = body[0];
                if flags & 1 != 0 {
                    if let Some(value) = le32(body, 1) {
                        entry.mtime = i64::from(value as i32);
                    }
                }
            }
            _ => {}
        }
        position = body_start + length;
    }
}

/// Decodes stored name bytes using libzip's default guess (UTF-8 flag or CP437).
pub(crate) fn decode_name(raw: &[u8], flags: u16) -> Vec<u8> {
    if flags & 0x0800 != 0 {
        return raw.to_vec();
    }
    if std::str::from_utf8(raw).is_ok() {
        return raw.to_vec();
    }
    raw.iter().map(|&byte| cp437_char(byte)).collect::<String>().into_bytes()
}

/// Decodes CP437 bytes into UTF-8 bytes.
pub(crate) fn decode_cp437(raw: &[u8]) -> Vec<u8> {
    raw.iter().map(|&byte| cp437_char(byte)).collect::<String>().into_bytes()
}

/// Maps one CP437 byte to its Unicode scalar, passing ASCII and unmapped bytes through.
fn cp437_char(byte: u8) -> char {
    if byte < 0x80 {
        return char::from(byte);
    }
    CP437[(byte - 0x80) as usize]
}

/// The CP437 upper half, indexed by `byte - 0x80`.
const CP437: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Parses the archive's central directory, including ZIP64 records.
pub(crate) fn parse(data: &[u8]) -> Result<Directory, ZipError> {
    let eocd = find_eocd(data).ok_or(ZipError::NotZip)?;
    let mut entry_count = le16(data, eocd + 10).map(u64::from).ok_or(ZipError::Truncated)?;
    let mut central_offset = le32(data, eocd + 16).map(u64::from).ok_or(ZipError::Truncated)?;
    let mut central_size = le32(data, eocd + 12).map(u64::from).ok_or(ZipError::Truncated)?;
    let comment_len = le16(data, eocd + 20).map(usize::from).ok_or(ZipError::Truncated)?;
    let comment_start = eocd + 22;
    let comment = data.get(comment_start..comment_start + comment_len)
        .ok_or(ZipError::Truncated)?.to_vec();

    // ZIP64 EOCD locator sits immediately before the regular EOCD.
    if eocd >= 20 && le32(data, eocd - 20) == Some(0x0706_4b50) {
        let zip64_offset = le64(data, eocd - 12).ok_or(ZipError::Truncated)? as usize;
        if le32(data, zip64_offset) == Some(0x0606_4b50) {
            let count_offset = zip64_offset.checked_add(32).ok_or(ZipError::Truncated)?;
            let size_offset = zip64_offset.checked_add(40).ok_or(ZipError::Truncated)?;
            let directory_offset = zip64_offset.checked_add(48).ok_or(ZipError::Truncated)?;
            entry_count = le64(data, count_offset).ok_or(ZipError::Truncated)?;
            central_size = le64(data, size_offset).ok_or(ZipError::Truncated)?;
            central_offset = le64(data, directory_offset).ok_or(ZipError::Truncated)?;
        }
    }

    let mut entries = Vec::with_capacity(entry_count.min(1 << 20) as usize);
    let mut position = usize::try_from(central_offset).map_err(|_| ZipError::Truncated)?;
    for _ in 0..entry_count {
        if le32(data, position) != Some(0x0201_4b50) {
            return Err(ZipError::Inconsistent);
        }
        let made_by = le16(data, position + 4).ok_or(ZipError::Truncated)?;
        let flags = le16(data, position + 8).ok_or(ZipError::Truncated)?;
        let method = le16(data, position + 10).ok_or(ZipError::Truncated)?;
        let dos_time = le16(data, position + 12).ok_or(ZipError::Truncated)?;
        let dos_date = le16(data, position + 14).ok_or(ZipError::Truncated)?;
        let crc = le32(data, position + 16).ok_or(ZipError::Truncated)?;
        let mut comp_size = u64::from(le32(data, position + 20).ok_or(ZipError::Truncated)?);
        let mut size = u64::from(le32(data, position + 24).ok_or(ZipError::Truncated)?);
        let name_len = usize::from(le16(data, position + 28).ok_or(ZipError::Truncated)?);
        let extra_len = usize::from(le16(data, position + 30).ok_or(ZipError::Truncated)?);
        let entry_comment_len = usize::from(le16(data, position + 32).ok_or(ZipError::Truncated)?);
        let external_attrs = le32(data, position + 38).ok_or(ZipError::Truncated)?;
        let mut local_offset = u64::from(le32(data, position + 42).ok_or(ZipError::Truncated)?);
        let name_start = position + 46;
        let name_raw = data.get(name_start..name_start + name_len)
            .ok_or(ZipError::Truncated)?.to_vec();
        let extra_start = name_start + name_len;
        let extra = data.get(extra_start..extra_start + extra_len).ok_or(ZipError::Truncated)?;
        let comment_start = extra_start + extra_len;
        let entry_comment = data.get(comment_start..comment_start + entry_comment_len)
            .ok_or(ZipError::Truncated)?.to_vec();

        let mut entry = RawEntry {
            name_raw,
            flags,
            method,
            crc,
            comp_size,
            size,
            comment: entry_comment,
            external_attrs,
            system: (made_by >> 8) as u8,
            local_offset,
            mtime: dos_to_unix(dos_date, dos_time),
            encryption: if flags & 1 != 0 { crate::consts::EM_TRAD_PKWARE } else { crate::consts::EM_NONE },
            aes_method: None,
        };
        let needs_size = size == 0xffff_ffff || comp_size == 0xffff_ffff;
        let needs_offset = local_offset == 0xffff_ffff;
        parse_extra(extra, &mut entry, needs_size, needs_offset);
        if size == 0xffff_ffff { size = entry.size; }
        if comp_size == 0xffff_ffff { comp_size = entry.comp_size; }
        if local_offset == 0xffff_ffff { local_offset = entry.local_offset; }
        entry.size = size;
        entry.comp_size = comp_size;
        entry.local_offset = local_offset;
        entries.push(entry);
        position = comment_start + entry_comment_len;
    }
    let directory_end = (central_offset as usize)
        .checked_add(central_size as usize)
        .ok_or(ZipError::Truncated)?;
    if position > directory_end {
        return Err(ZipError::Truncated);
    }
    let torrentzip = comment.starts_with(b"TORRENTZIPPED-");
    Ok(Directory { entries, comment, torrentzip })
}

/// Validates every central-directory entry against its local header and data range.
///
/// libzip's `ZIP_CHECKCONS` checks archive structure, not payload CRCs; a CRC-corrupt
/// entry must still open so a later stream read can report the CRC error. This pass
/// catches mismatched names/methods, broken local offsets and truncated data blocks.
pub(crate) fn check_consistency(data: &[u8], directory: &Directory) -> Result<(), ZipError> {
    for entry in &directory.entries {
        let offset = usize::try_from(entry.local_offset).map_err(|_| ZipError::Truncated)?;
        if le32(data, offset) != Some(0x0403_4b50) {
            return Err(ZipError::Inconsistent);
        }
        let method = le16(data, offset.checked_add(8).ok_or(ZipError::Truncated)?)
            .ok_or(ZipError::Truncated)?;
        if method != entry.method {
            return Err(ZipError::Inconsistent);
        }
        let name_len = usize::from(
            le16(data, offset.checked_add(26).ok_or(ZipError::Truncated)?)
                .ok_or(ZipError::Truncated)?,
        );
        let extra_len = usize::from(
            le16(data, offset.checked_add(28).ok_or(ZipError::Truncated)?)
                .ok_or(ZipError::Truncated)?,
        );
        let name_start = offset.checked_add(30).ok_or(ZipError::Truncated)?;
        let name_end = name_start.checked_add(name_len).ok_or(ZipError::Truncated)?;
        let extra_end = name_end.checked_add(extra_len).ok_or(ZipError::Truncated)?;
        let name = data.get(name_start..name_end).ok_or(ZipError::Truncated)?;
        if name != entry.name_raw.as_slice() {
            return Err(ZipError::Inconsistent);
        }
        let data_start = extra_end;
        let data_end = data_start
            .checked_add(usize::try_from(entry.comp_size).map_err(|_| ZipError::Truncated)?)
            .ok_or(ZipError::Truncated)?;
        if data_end > data.len() {
            return Err(ZipError::Truncated);
        }
    }
    Ok(())
}

/// Sets the encrypted flag on the named entries' local and central records.
///
/// Used after a ZipCrypto raw copy: the zip writer does not preserve the encrypted bit,
/// but the payload is already encrypted and sized correctly.
pub(crate) fn patch_encrypted_flags(data: &mut [u8], names: &[Vec<u8>]) -> Result<(), ZipError> {
    if names.is_empty() {
        return Ok(());
    }
    let eocd = find_eocd(data).ok_or(ZipError::NotZip)?;
    let count = le16(data, eocd + 10).map(u32::from).ok_or(ZipError::Truncated)?;
    let mut position = le32(data, eocd + 16).map(usize::try_from).transpose()
        .map_err(|_| ZipError::Truncated)?.ok_or(ZipError::Truncated)?;
    for _ in 0..count {
        if le32(data, position) != Some(0x0201_4b50) {
            return Err(ZipError::Inconsistent);
        }
        let name_len = usize::from(le16(data, position + 28).ok_or(ZipError::Truncated)?);
        let extra_len = usize::from(le16(data, position + 30).ok_or(ZipError::Truncated)?);
        let comment_len = usize::from(le16(data, position + 32).ok_or(ZipError::Truncated)?);
        let local_offset = le32(data, position + 42).map(usize::try_from).transpose()
            .map_err(|_| ZipError::Truncated)?.ok_or(ZipError::Truncated)?;
        let name_start = position + 46;
        let name = data.get(name_start..name_start + name_len).ok_or(ZipError::Truncated)?;
        if names.iter().any(|wanted| wanted.as_slice() == name) {
            let central_flags = le16(data, position + 8).ok_or(ZipError::Truncated)? | 1;
            data.get_mut(position + 8..position + 10)
                .ok_or(ZipError::Truncated)?.copy_from_slice(&central_flags.to_le_bytes());
            if le32(data, local_offset) != Some(0x0403_4b50) {
                return Err(ZipError::Inconsistent);
            }
            let local_flags = le16(data, local_offset + 6).ok_or(ZipError::Truncated)? | 1;
            data.get_mut(local_offset + 6..local_offset + 8)
                .ok_or(ZipError::Truncated)?.copy_from_slice(&local_flags.to_le_bytes());
        }
        position = name_start + name_len + extra_len + comment_len;
    }
    Ok(())
}

/// Returns the payload offset behind one entry's local file header.
pub(crate) fn local_payload_offset(data: &[u8], entry: &RawEntry) -> Option<usize> {
    let header = usize::try_from(entry.local_offset).ok()?;
    if le32(data, header) != Some(0x0403_4b50) {
        return None;
    }
    let name_len = usize::from(le16(data, header + 26)?);
    let extra_len = usize::from(le16(data, header + 28)?);
    Some(header + 30 + name_len + extra_len)
}
