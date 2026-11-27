//! Purpose:
//! Public raw-ZIP reader used by the `elephc-zip` bridge and `ZipArchive`.
//!
//! Called from:
//! - `crates/elephc-zip` through the public `open_zip_data` /
//!   `extract_zip_entry` / `locate_zip_entry` API.
//!
//! Key details:
//! - Unlike the PHAR reader, this path does not require or verify a PHAR
//!   signature and does not hide `.phar/*` control entries.
//! - ZIP64 sizes/offsets, stored/deflated payloads, and data descriptors are
//!   delegated to the existing `zip_read` primitives.
//! - Every central-directory field needed by a re-writing client (timestamps,
//!   CRC, comments, external attributes) is preserved on `ZipEntryInfo`.

use crate::zip_read::{
    apply_zip64_central_extra, decode_zip_local_entry, find_zip_eocd, zip_entry_crypto,
    zip_eocd_info,
};
use crate::{le16, le32};

/// One entry from a ZIP central directory.
#[derive(Clone, Debug)]
pub struct ZipEntryInfo {
    /// Raw entry name bytes.
    pub name: Vec<u8>,
    /// Compression method (0 = store, 8 = deflate).
    pub method: u16,
    /// General-purpose bit flags.
    pub flags: u16,
    /// MS-DOS modification time.
    pub dos_time: u16,
    /// MS-DOS modification date.
    pub dos_date: u16,
    /// CRC-32 of the uncompressed payload.
    pub crc32: u32,
    /// Compressed payload size.
    pub compressed_size: usize,
    /// Uncompressed payload size.
    pub uncompressed_size: usize,
    /// Offset of the local file header.
    pub local_offset: usize,
    /// Entry comment bytes.
    pub comment: Vec<u8>,
    /// External file attributes (Unix mode in the high 16 bits for Unix-made archives).
    pub external_attrs: u32,
    /// Whether the entry uses traditional ZipCrypto encryption.
    pub encrypted: bool,
    /// ZipCrypto check byte.
    pub check_byte: u8,
}

/// A parsed ZIP archive held in memory.
#[derive(Clone, Debug)]
pub struct ZipArchiveData {
    /// Original archive bytes.
    pub data: Vec<u8>,
    /// Central-directory entries in archive order.
    pub entries: Vec<ZipEntryInfo>,
    /// Archive comment bytes.
    pub comment: Vec<u8>,
}

/// Parses a raw ZIP/ZIP64 archive without PHAR authentication or filtering.
pub fn open_zip_data(data: &[u8]) -> Option<ZipArchiveData> {
    let eocd = find_zip_eocd(data)?;
    let (entry_count, central_dir_offset) = zip_eocd_info(data)?;
    let comment_len = le16(data, eocd + 20)? as usize;
    let comment_start = eocd.checked_add(22)?;
    let comment = data
        .get(comment_start..comment_start.checked_add(comment_len)?)?
        .to_vec();
    let mut entries = Vec::with_capacity(entry_count.min(1 << 16));
    let mut p = central_dir_offset;
    for _ in 0..entry_count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        let flags = le16(data, p + 8)?;
        let method = le16(data, p + 10)?;
        let dos_time = le16(data, p + 12)?;
        let dos_date = le16(data, p + 14)?;
        let crc32 = le32(data, p + 16)?;
        let mut compressed_size = le32(data, p + 20)? as usize;
        let mut uncompressed_size = le32(data, p + 24)? as usize;
        let name_len = le16(data, p + 28)? as usize;
        let extra_len = le16(data, p + 30)? as usize;
        let entry_comment_len = le16(data, p + 32)? as usize;
        let external_attrs = le32(data, p + 38)?;
        let mut local_offset = le32(data, p + 42)? as usize;
        let name_start = p.checked_add(46)?;
        let name = data
            .get(name_start..name_start.checked_add(name_len)?)?
            .to_vec();
        apply_zip64_central_extra(
            data,
            name_start.checked_add(name_len)?,
            extra_len,
            &mut uncompressed_size,
            &mut compressed_size,
            &mut local_offset,
        )?;
        let comment_start = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?;
        let comment = data
            .get(comment_start..comment_start.checked_add(entry_comment_len)?)?
            .to_vec();
        let (encrypted, check_byte) = zip_entry_crypto(data, p)?;
        entries.push(ZipEntryInfo {
            name,
            method,
            flags,
            dos_time,
            dos_date,
            crc32,
            compressed_size,
            uncompressed_size,
            local_offset,
            comment,
            external_attrs,
            encrypted,
            check_byte,
        });
        p = comment_start.checked_add(entry_comment_len)?;
    }
    Some(ZipArchiveData {
        data: data.to_vec(),
        entries,
        comment,
    })
}

/// Extracts one entry by central-directory index.
pub fn extract_zip_entry(archive: &ZipArchiveData, index: usize) -> Option<Vec<u8>> {
    let entry = archive.entries.get(index)?;
    decode_zip_local_entry(
        &archive.data,
        entry.local_offset,
        entry.method,
        entry.compressed_size,
        entry.uncompressed_size,
        entry.encrypted,
        entry.check_byte,
    )
}

/// Returns one entry's raw (still compressed/encrypted) payload bytes.
pub fn zip_entry_compressed_payload(
    archive: &ZipArchiveData,
    index: usize,
) -> Option<Vec<u8>> {
    let entry = archive.entries.get(index)?;
    if le32(&archive.data, entry.local_offset)? != 0x0403_4b50 {
        return None;
    }
    let name_len = le16(&archive.data, entry.local_offset + 26)? as usize;
    let extra_len = le16(&archive.data, entry.local_offset + 28)? as usize;
    let start = entry
        .local_offset
        .checked_add(30)?
        .checked_add(name_len)?
        .checked_add(extra_len)?;
    archive
        .data
        .get(start..start.checked_add(entry.compressed_size)?)
        .map(<[u8]>::to_vec)
}

/// Returns the index of the first entry whose raw name matches `name`.
pub fn locate_zip_entry(archive: &ZipArchiveData, name: &[u8]) -> Option<usize> {
    archive
        .entries
        .iter()
        .position(|entry| entry.name == name)
}

/// Converts an MS-DOS (time, date) pair to a Unix timestamp.
pub fn dos_to_unix(dos_time: u16, dos_date: u16) -> i64 {
    let year = 1980 + i64::from((dos_date >> 9) & 0x7f);
    let month = i64::from((dos_date >> 5) & 0x0f).clamp(1, 12);
    let day = i64::from(dos_date & 0x1f).clamp(1, 31);
    let hour = i64::from((dos_time >> 11) & 0x1f);
    let minute = i64::from((dos_time >> 5) & 0x3f);
    let second = i64::from(dos_time & 0x1f) * 2;
    days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second
}

/// Converts a Unix timestamp to an MS-DOS (time, date) pair.
pub fn unix_to_dos(timestamp: i64) -> (u16, u16) {
    let (year, month, day, hour, minute, second) = civil_from_unix(timestamp);
    let year = year.clamp(1980, 2107);
    let dos_date = ((year - 1980) as u16) << 9 | (month as u16) << 5 | day as u16;
    let dos_time = (hour as u16) << 11 | (minute as u16) << 5 | (second / 2) as u16;
    (dos_time, dos_date)
}

/// Returns the number of days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Splits a Unix timestamp into Gregorian `(year, month, day, hour, minute, second)`.
fn civil_from_unix(timestamp: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 { month_prime + 3 } else { month_prime - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (
        year,
        month,
        day,
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60,
    )
}
