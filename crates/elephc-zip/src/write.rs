//! Purpose:
//! Serializes the in-memory `Archive` model back into a ZIP byte stream.
//!
//! Called from:
//! - `crate::model::Archive::close_string` when an archive was modified.
//!
//! Key details:
//! - Unchanged entries reuse their original compressed payload and CRC, so
//!   unsupported/encrypted entries survive an unrelated edit untouched.
//! - Only store (0) and deflate (8) methods are emitted; archives that would
//!   need ZIP64 are rejected rather than silently truncated.

use crate::model::{Archive, METHOD_DEFLATE, METHOD_STORE};

/// Serializes `archive` into a regular (non-ZIP64) ZIP file.
pub fn serialize(archive: &Archive) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in &archive.entries {
        let compressed = entry.compressed.as_deref()?;
        if entry.method != METHOD_STORE && entry.method != METHOD_DEFLATE {
            return None;
        }
        let compressed_size = u32::try_from(compressed.len()).ok()?;
        let uncompressed_size = u32::try_from(entry.uncompressed_size).ok()?;
        let local_offset = u32::try_from(out.len()).ok()?;
        let flags = entry_flags(entry);
        write_local_header(
            &mut out,
            flags,
            entry.method,
            entry.dos_time,
            entry.dos_date,
            entry.crc32,
            compressed_size,
            uncompressed_size,
            &entry.name,
            compressed,
        )?;
        write_central_header(
            &mut central,
            flags,
            entry.method,
            entry.dos_time,
            entry.dos_date,
            entry.crc32,
            compressed_size,
            uncompressed_size,
            local_offset,
            &entry.name,
            &entry.comment,
            entry.external_attrs,
        )?;
    }
    let central_offset = u32::try_from(out.len()).ok()?;
    let entry_count = u16::try_from(archive.entries.len()).ok()?;
    out.extend_from_slice(&central);
    let central_size = u32::try_from(central.len()).ok()?;
    let comment = &archive.comment[..archive.comment.len().min(0xffff)];
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&entry_count.to_le_bytes());
    out.extend_from_slice(&entry_count.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
    out.extend_from_slice(comment);
    Some(out)
}

/// Returns the general-purpose flags to emit, adding the UTF-8 bit when needed
/// and dropping the data-descriptor bit because sizes are written up front.
fn entry_flags(entry: &crate::model::Entry) -> u16 {
    let mut flags = entry.flags & !0x0008;
    if !entry.name.is_ascii() && std::str::from_utf8(&entry.name).is_ok() {
        flags |= 0x0800;
    }
    flags
}

/// Appends one local file header followed by its compressed payload.
#[allow(clippy::too_many_arguments)]
fn write_local_header(
    out: &mut Vec<u8>,
    flags: u16,
    method: u16,
    dos_time: u16,
    dos_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    name: &[u8],
    payload: &[u8],
) -> Option<()> {
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&dos_time.to_le_bytes());
    out.extend_from_slice(&dos_date.to_le_bytes());
    out.extend_from_slice(&crc32.to_le_bytes());
    out.extend_from_slice(&compressed_size.to_le_bytes());
    out.extend_from_slice(&uncompressed_size.to_le_bytes());
    out.extend_from_slice(&u16::try_from(name.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(payload);
    Some(())
}

/// Appends one central-directory record.
#[allow(clippy::too_many_arguments)]
fn write_central_header(
    out: &mut Vec<u8>,
    flags: u16,
    method: u16,
    dos_time: u16,
    dos_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    local_offset: u32,
    name: &[u8],
    comment: &[u8],
    external_attrs: u32,
) -> Option<()> {
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&0x031eu16.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&dos_time.to_le_bytes());
    out.extend_from_slice(&dos_date.to_le_bytes());
    out.extend_from_slice(&crc32.to_le_bytes());
    out.extend_from_slice(&compressed_size.to_le_bytes());
    out.extend_from_slice(&uncompressed_size.to_le_bytes());
    out.extend_from_slice(&u16::try_from(name.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&u16::try_from(comment.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&external_attrs.to_le_bytes());
    out.extend_from_slice(&local_offset.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(comment);
    Some(())
}
