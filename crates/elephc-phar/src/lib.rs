//! Purpose:
//! Pure-Rust archive bridge for elephc's `phar://` runtime paths.
//! Extracts native PHAR, tar-based PHAR, and zip-based PHAR entries, and writes
//! native PHAR entries through a small C ABI so generated assembly does not
//! duplicate archive parsers or manifest writers.
//!
//! Called from:
//! - Compiled PHP program assembly through the `_elephc_phar_extract_url_fn`
//!   and `_elephc_phar_put_entry_fn` slots.
//! - `src/codegen/builtins/io/phar_stream.rs` for literal compile-time reads.
//! - `cargo test -p elephc-phar` for in-isolation validation.
//!
//! Key details:
//! - Returned FFI pointers reference a process-global buffer and remain valid
//!   until the next `elephc_phar_extract_url` call.
//! - ZIP64, encrypted ZIP entries, ZIP data descriptors, tar/zip writes, and
//!   compressed PHAR writes are intentionally unsupported.

use std::io::Read;
use std::sync::{Mutex, OnceLock};

const PHAR_FLAG_GZIP: u32 = 0x0000_1000;
const PHAR_FLAG_BZIP2: u32 = 0x0000_2000;
const PHAR_HDR_SIGNATURE: u32 = 0x0001_0000;
const PHAR_FILE_MODE_0644: u32 = 0x0000_01a4;
const PHAR_SHA1_SIGNATURE_TYPE: u32 = 0x0000_0002;
const ZIP_METHOD_STORE: u16 = 0;
const ZIP_METHOD_DEFLATE: u16 = 8;
const ZIP_FLAG_DATA_DESCRIPTOR: u16 = 0x0008;

static EXTRACT_BUFFER: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();

#[derive(Clone)]
struct NativePharEntry {
    name: Vec<u8>,
    payload: Vec<u8>,
}

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
    let archive = std::fs::read(archive_path).ok()?;
    extract_entry_bytes(&archive, entry)
}

/// Extracts `entry` from already-loaded archive bytes.
///
/// Native PHAR is tried first because it has an explicit manifest and may have
/// arbitrary stubs before the payload. Plain ZIP and TAR containers are then
/// tried by signature/layout.
pub fn extract_entry_bytes(archive: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    parse_native_phar_entry(archive, entry)
        .or_else(|| parse_zip_entry(archive, entry))
        .or_else(|| parse_tar_entry(archive, entry))
}

/// Inserts or replaces one uncompressed entry in a native PHAR archive on disk.
///
/// Missing archives are created. Existing native PHAR archives are read,
/// decoded, rewritten as uncompressed native PHAR, and SHA1-signed. Existing
/// tar/zip containers are intentionally rejected because tar/zip writes are not
/// part of elephc's supported `phar://` write surface.
pub fn put_native_entry(
    archive_path: &[u8],
    entry_name: &[u8],
    payload: &[u8],
) -> Option<usize> {
    if entry_name.is_empty() {
        return None;
    }
    let archive_path = std::str::from_utf8(archive_path).ok()?;
    let path = std::path::Path::new(archive_path);
    let mut entries = if path.exists() {
        let archive = std::fs::read(path).ok()?;
        parse_native_phar_entries(&archive)?
    } else {
        Vec::new()
    };
    upsert_native_entry(&mut entries, entry_name, payload);
    let archive = build_native_phar_archive(&entries)?;
    std::fs::write(path, archive).ok()?;
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
    put_native_entry(archive_path, entry_name, payload)
}

/// C ABI wrapper around [`extract_url_bytes`].
///
/// Returns a pointer to a stable process-global buffer and writes the byte
/// length into `out_len`. Returns null and writes zero on any failure.
///
/// # Safety
/// `url_ptr` must be valid for `url_len` bytes unless `url_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_extract_url(
    url_ptr: *const u8,
    url_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| extract_url_bytes(slice(url_ptr, url_len))) {
        Ok(Some(bytes)) => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`put_native_entry`].
///
/// Returns the written payload length on success and `usize::MAX` on failure.
/// The archive is always a native PHAR after a successful write.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `entry_ptr` must not describe an empty entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_put_entry(
    archive_ptr: *const u8,
    archive_len: usize,
    entry_ptr: *const u8,
    entry_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        put_native_entry(
            slice(archive_ptr, archive_len),
            slice(entry_ptr, entry_len),
            slice(data_ptr, data_len),
        )
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// C ABI wrapper around [`put_url_bytes`].
///
/// Returns the written payload length on success and `usize::MAX` on failure.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `url_ptr` must point to a complete `phar://archive/entry` URL.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_put_url(
    url_ptr: *const u8,
    url_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        put_url_bytes(slice(url_ptr, url_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// Builds a byte slice from a C pointer and byte length.
///
/// A zero length never dereferences the pointer, so null plus zero is accepted.
unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Stores extracted bytes in the process-global result buffer and returns its pointer.
fn publish_result(bytes: Vec<u8>, out_len: *mut usize) -> *const u8 {
    let mut buffer = EXTRACT_BUFFER
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("elephc_phar extract buffer poisoned");
    buffer.clear();
    buffer.extend_from_slice(&bytes);
    write_len(out_len, buffer.len());
    if buffer.is_empty() {
        b"".as_ptr()
    } else {
        buffer.as_ptr()
    }
}

/// Writes an output length through the optional C pointer.
fn write_len(out_len: *mut usize, len: usize) {
    if !out_len.is_null() {
        unsafe {
            *out_len = len;
        }
    }
}

/// Splits `phar://` URL body bytes into an existing archive path and inner entry name.
fn split_archive_entry(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    for (i, &byte) in rest.iter().enumerate() {
        if byte != b'/' || i == 0 || i + 1 >= rest.len() {
            continue;
        }
        let candidate = std::str::from_utf8(&rest[..i]).ok()?;
        if std::path::Path::new(candidate).is_file() {
            return Some((&rest[..i], &rest[i + 1..]));
        }
    }
    None
}

/// Splits `phar://` URL body bytes for writes, including missing archives.
fn split_write_url_entry(rest: &[u8]) -> Option<(&[u8], &[u8])> {
    if let Some(idx) = find_subslice(rest, b".phar/") {
        let split = idx.checked_add(b".phar".len())?;
        return Some((rest.get(..split)?, rest.get(split + 1..)?));
    }
    let idx = rest.iter().rposition(|&byte| byte == b'/')?;
    if idx == 0 || idx + 1 >= rest.len() {
        return None;
    }
    Some((rest.get(..idx)?, rest.get(idx + 1..)?))
}

/// Parses a native PHAR archive and returns a decoded entry payload.
fn parse_native_phar_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    parse_native_phar_entries(data)?
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a native PHAR archive and returns every decoded entry payload.
fn parse_native_phar_entries(data: &[u8]) -> Option<Vec<NativePharEntry>> {
    let halt = b"__HALT_COMPILER();";
    let halt_idx = find_subslice(data, halt)?;
    let mut p = halt_idx + halt.len();
    for &ch in &[b' ', b'?', b'>', b'\r', b'\n'] {
        if data.get(p) == Some(&ch) {
            p += 1;
        }
    }

    let manifest_start = p;
    let manifest_len = le32(data, manifest_start)? as usize;
    let data_section = manifest_start.checked_add(4)?.checked_add(manifest_len)?;
    let num_files = le32(data, manifest_start + 4)?;
    let mut q = manifest_start + 8 + 2 + 4;
    let alias_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(alias_len)?;
    let meta_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(meta_len)?;

    let mut data_offset = 0usize;
    let mut entries = Vec::with_capacity(num_files as usize);
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
        q = q.checked_add(4)?.checked_add(entry_meta_len)?;

        let start = data_section.checked_add(data_offset)?;
        let stored = data.get(start..start.checked_add(compressed)?)?;
        let payload = decode_phar_payload(stored, flags, uncompressed)?;
        entries.push(NativePharEntry {
            name: name.to_vec(),
            payload,
        });
        data_offset = data_offset.checked_add(compressed)?;
    }
    Some(entries)
}

/// Decodes a native PHAR entry payload according to its per-entry flags.
fn decode_phar_payload(stored: &[u8], flags: u32, uncompressed: usize) -> Option<Vec<u8>> {
    if flags & PHAR_FLAG_GZIP != 0 {
        let mut out = Vec::with_capacity(uncompressed);
        let mut decoder = flate2::read::DeflateDecoder::new(stored);
        decoder.read_to_end(&mut out).ok()?;
        (out.len() == uncompressed).then_some(out)
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        let mut out = Vec::with_capacity(uncompressed);
        let mut decoder = bzip2_rs::DecoderReader::new(stored);
        decoder.read_to_end(&mut out).ok()?;
        (out.len() == uncompressed).then_some(out)
    } else {
        Some(stored.to_vec())
    }
}

/// Inserts `payload` under `entry_name`, replacing an existing entry with the same name.
fn upsert_native_entry(entries: &mut Vec<NativePharEntry>, entry_name: &[u8], payload: &[u8]) {
    if let Some(existing) = entries.iter_mut().find(|entry| entry.name == entry_name) {
        existing.payload.clear();
        existing.payload.extend_from_slice(payload);
    } else {
        entries.push(NativePharEntry {
            name: entry_name.to_vec(),
            payload: payload.to_vec(),
        });
    }
}

/// Builds a SHA1-signed native PHAR archive from decoded, uncompressed entries.
fn build_native_phar_archive(entries: &[NativePharEntry]) -> Option<Vec<u8>> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&u32::try_from(entries.len()).ok()?.to_le_bytes());
    manifest.extend_from_slice(&[0x11, 0x00]);
    manifest.extend_from_slice(&PHAR_HDR_SIGNATURE.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
    for entry in entries {
        let name_len = u32::try_from(entry.name.len()).ok()?;
        let payload_len = u32::try_from(entry.payload.len()).ok()?;
        manifest.extend_from_slice(&name_len.to_le_bytes());
        manifest.extend_from_slice(&entry.name);
        manifest.extend_from_slice(&payload_len.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&payload_len.to_le_bytes());
        manifest.extend_from_slice(&crc32(&entry.payload).to_le_bytes());
        manifest.extend_from_slice(&PHAR_FILE_MODE_0644.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
    out.extend_from_slice(&u32::try_from(manifest.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&manifest);
    for entry in entries {
        out.extend_from_slice(&entry.payload);
    }
    append_sha1_signature(&mut out);
    Some(out)
}

/// Appends PHP's raw-SHA1 PHAR signature trailer to `archive`.
fn append_sha1_signature(archive: &mut Vec<u8>) {
    use sha1::{Digest, Sha1};

    let digest = Sha1::digest(&archive);
    archive.extend_from_slice(&digest);
    archive.extend_from_slice(&PHAR_SHA1_SIGNATURE_TYPE.to_le_bytes());
    archive.extend_from_slice(b"GBMB");
}

/// Computes PHP-compatible reflected CRC32 for a PHAR entry payload.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Parses a ZIP archive central directory and returns a store/deflate entry.
fn parse_zip_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    let eocd = find_zip_eocd(data)?;
    let entry_count = le16(data, eocd + 10)? as usize;
    let central_dir_offset = le32(data, eocd + 16)? as usize;
    let mut p = central_dir_offset;
    for _ in 0..entry_count {
        if le32(data, p)? != 0x0201_4b50 {
            return None;
        }
        let flags = le16(data, p + 8)?;
        if flags & ZIP_FLAG_DATA_DESCRIPTOR != 0 {
            return None;
        }
        let method = le16(data, p + 10)?;
        let compressed_size = le32(data, p + 20)? as usize;
        let uncompressed_size = le32(data, p + 24)? as usize;
        let name_len = le16(data, p + 28)? as usize;
        let extra_len = le16(data, p + 30)? as usize;
        let comment_len = le16(data, p + 32)? as usize;
        let local_offset = le32(data, p + 42)? as usize;
        let name_start = p + 46;
        let name = data.get(name_start..name_start.checked_add(name_len)?)?;
        if name == entry {
            return decode_zip_local_entry(
                data,
                local_offset,
                method,
                compressed_size,
                uncompressed_size,
            );
        }
        p = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(comment_len)?;
    }
    None
}

/// Finds the ZIP end-of-central-directory record.
fn find_zip_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let start = data.len().saturating_sub(65_557);
    (start..=data.len() - 22)
        .rev()
        .find(|&i| data.get(i..i + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
}

/// Decodes a ZIP local file payload using sizes from its central directory.
fn decode_zip_local_entry(
    data: &[u8],
    local_offset: usize,
    method: u16,
    compressed_size: usize,
    uncompressed_size: usize,
) -> Option<Vec<u8>> {
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
    match method {
        ZIP_METHOD_STORE => Some(stored.to_vec()),
        ZIP_METHOD_DEFLATE => {
            let mut out = Vec::with_capacity(uncompressed_size);
            let mut decoder = flate2::read::DeflateDecoder::new(stored);
            decoder.read_to_end(&mut out).ok()?;
            (out.len() == uncompressed_size).then_some(out)
        }
        _ => None,
    }
}

/// Parses a POSIX tar archive and returns a regular-file entry.
fn parse_tar_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    let mut p = 0usize;
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&b| b == 0) {
            return None;
        }
        let size = parse_tar_octal(&header[124..136])?;
        let payload_start = p.checked_add(512)?;
        let payload_end = payload_start.checked_add(size)?;
        let payload = data.get(payload_start..payload_end)?;
        let typeflag = header[156];
        if (typeflag == 0 || typeflag == b'0') && tar_entry_name(header).as_deref() == Some(entry) {
            return Some(payload.to_vec());
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    None
}

/// Builds the full tar path from the `prefix` and `name` header fields.
fn tar_entry_name(header: &[u8]) -> Option<Vec<u8>> {
    let name = trim_nul_and_space(header.get(0..100)?);
    let prefix = trim_nul_and_space(header.get(345..500)?);
    if prefix.is_empty() {
        Some(name.to_vec())
    } else {
        let mut out = Vec::with_capacity(prefix.len() + 1 + name.len());
        out.extend_from_slice(prefix);
        out.push(b'/');
        out.extend_from_slice(name);
        Some(out)
    }
}

/// Parses a tar octal integer field.
fn parse_tar_octal(field: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut saw_digit = false;
    for &byte in field {
        if byte == 0 || byte == b' ' {
            if saw_digit {
                break;
            }
            continue;
        }
        if !(b'0'..=b'7').contains(&byte) {
            return None;
        }
        saw_digit = true;
        value = value.checked_mul(8)?.checked_add((byte - b'0') as usize)?;
    }
    Some(value)
}

/// Rounds a tar payload length up to the next 512-byte block count.
fn round_up_to_512(len: usize) -> Option<usize> {
    len.checked_add(511).map(|n| (n / 512) * 512)
}

/// Trims a NUL-terminated, space-padded archive field.
fn trim_nul_and_space(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut end = end;
    while end > 0 && bytes[end - 1] == b' ' {
        end -= 1;
    }
    &bytes[..end]
}

/// Reads a little-endian `u16` from `data`.
fn le16(data: &[u8], off: usize) -> Option<u16> {
    let b = data.get(off..off + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

/// Reads a little-endian `u32` from `data`.
fn le32(data: &[u8], off: usize) -> Option<u32> {
    let b = data.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Returns the offset of `needle` in `hay`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// Builds a minimal native PHAR fixture with uncompressed entries.
    fn build_native_phar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut manifest = Vec::new();
        manifest.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        manifest.extend_from_slice(&[0x11, 0x00]);
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        for (name, content) in entries {
            manifest.extend_from_slice(&(name.len() as u32).to_le_bytes());
            manifest.extend_from_slice(name.as_bytes());
            manifest.extend_from_slice(&(content.len() as u32).to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&(content.len() as u32).to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&0x0000_01a4u32.to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&manifest);
        for (_, content) in entries {
            out.extend_from_slice(content);
        }
        out
    }

    /// Builds a small tar archive with regular-file entries.
    fn build_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, content) in entries {
            let mut header = [0u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            let size = format!("{:011o}\0", content.len());
            header[124..124 + size.len()].copy_from_slice(size.as_bytes());
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            for byte in &mut header[148..156] {
                *byte = b' ';
            }
            let checksum: u32 = header.iter().map(|&b| b as u32).sum();
            let checksum = format!("{:06o}\0 ", checksum);
            header[148..156].copy_from_slice(checksum.as_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(content);
            out.resize(out.len() + round_up_to_512(content.len()).unwrap() - content.len(), 0);
        }
        out.extend_from_slice(&[0u8; 1024]);
        out
    }

    /// Builds a ZIP archive with central-directory records.
    fn build_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, content, deflate) in entries {
            let local_offset = out.len() as u32;
            let stored = if *deflate {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(content).unwrap();
                encoder.finish().unwrap()
            } else {
                content.to_vec()
            };
            let method = if *deflate { ZIP_METHOD_DEFLATE } else { ZIP_METHOD_STORE };
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&stored);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            central.extend_from_slice(&(content.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let central_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// Verifies native PHAR manifest extraction.
    #[test]
    fn extracts_native_phar_entry() {
        let archive = build_native_phar(&[("a.txt", b"alpha"), ("dir/b.txt", b"bravo")]);
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/b.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies tar container extraction.
    #[test]
    fn extracts_tar_entry() {
        let archive = build_tar(&[("a.txt", b"alpha"), ("dir/b.txt", b"bravo")]);
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/b.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies ZIP store and deflate extraction.
    #[test]
    fn extracts_zip_entries() {
        let archive = build_zip(&[
            ("plain.txt", b"stored", false),
            ("deflated.txt", b"deflated payload", true),
        ]);
        assert_eq!(
            extract_entry_bytes(&archive, b"plain.txt").as_deref(),
            Some(&b"stored"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"deflated.txt").as_deref(),
            Some(&b"deflated payload"[..])
        );
    }

    /// Verifies native PHAR writes preserve existing entries and update duplicates.
    #[test]
    fn writes_and_updates_native_phar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_put_entry_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_native_entry(path_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(
            put_native_entry(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
        assert_eq!(
            put_native_entry(path_bytes.as_bytes(), b"one.txt", b"updated"),
            Some(7)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"updated"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }

    /// Verifies full phar:// URL writes split archive and entry names at run time.
    #[test]
    fn writes_native_phar_entries_from_url() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_put_url_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let url = format!("phar://{}/one.txt", path.display());
        assert_eq!(put_url_bytes(url.as_bytes(), b"alpha"), Some(5));
        let nested_url = format!("phar://{}/dir/two.txt", path.display());
        assert_eq!(put_url_bytes(nested_url.as_bytes(), b"bravo"), Some(5));
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"alpha"[..])
        );
        assert_eq!(
            extract_entry_bytes(&archive, b"dir/two.txt").as_deref(),
            Some(&b"bravo"[..])
        );
    }
}
