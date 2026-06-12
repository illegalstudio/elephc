//! Purpose:
//! Pure-Rust archive bridge for elephc's `phar://` runtime paths.
//! Extracts native PHAR, tar-based PHAR, and zip-based PHAR entries, and writes
//! archive entries through a small C ABI so generated assembly does not
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
//! - Writes preserve the archive family for existing native PHAR, tar, and ZIP
//!   archives. Native PHAR gzip/bzip2 entries keep their compression when
//!   replaced. ZIP writes emit stored entries; ZIP64, encrypted ZIP entries,
//!   ZIP data descriptors, and explicit compression-control APIs are
//!   intentionally unsupported.

use std::io::{Read, Write};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PharCompression {
    None,
    Gzip,
    Bzip2,
}

#[derive(Clone)]
struct ArchiveEntry {
    name: Vec<u8>,
    payload: Vec<u8>,
    compression: PharCompression,
}

#[derive(Clone, Copy)]
enum ArchiveFormat {
    NativePhar,
    Tar,
    Zip,
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
    let (mut entries, format) = if path.exists() {
        let archive = std::fs::read(path).ok()?;
        parse_archive_entries(&archive)?
    } else {
        (Vec::new(), format_for_new_archive_path(path))
    };
    upsert_entry(&mut entries, entry_name, payload);
    let archive = build_archive(&entries, format)?;
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
    put_entry_bytes(archive_path, entry_name, payload)
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

/// C ABI wrapper around [`put_entry_bytes`].
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
        put_entry_bytes(
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
    for suffix in [b".phar/".as_slice(), b".tar/".as_slice(), b".zip/".as_slice()] {
        if let Some(idx) = find_subslice(rest, suffix) {
            let split = idx.checked_add(suffix.len().checked_sub(1)?)?;
            return Some((rest.get(..split)?, rest.get(split + 1..)?));
        }
    }
    let idx = rest.iter().rposition(|&byte| byte == b'/')?;
    if idx == 0 || idx + 1 >= rest.len() {
        return None;
    }
    Some((rest.get(..idx)?, rest.get(idx + 1..)?))
}

/// Parses archive bytes into decoded entries and reports the archive family.
fn parse_archive_entries(data: &[u8]) -> Option<(Vec<ArchiveEntry>, ArchiveFormat)> {
    parse_native_phar_entries(data)
        .map(|entries| (entries, ArchiveFormat::NativePhar))
        .or_else(|| parse_zip_entries(data).map(|entries| (entries, ArchiveFormat::Zip)))
        .or_else(|| parse_tar_entries(data).map(|entries| (entries, ArchiveFormat::Tar)))
}

/// Selects the archive family for a missing output path.
fn format_for_new_archive_path(path: &std::path::Path) -> ArchiveFormat {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("tar") => ArchiveFormat::Tar,
        Some(ext) if ext.eq_ignore_ascii_case("zip") => ArchiveFormat::Zip,
        _ => ArchiveFormat::NativePhar,
    }
}

/// Builds an archive in the selected output family.
fn build_archive(entries: &[ArchiveEntry], format: ArchiveFormat) -> Option<Vec<u8>> {
    match format {
        ArchiveFormat::NativePhar => build_native_phar_archive(entries),
        ArchiveFormat::Tar => build_tar_archive(entries),
        ArchiveFormat::Zip => build_zip_archive(entries),
    }
}

/// Parses a native PHAR archive and returns a decoded entry payload.
fn parse_native_phar_entry(data: &[u8], entry: &[u8]) -> Option<Vec<u8>> {
    parse_native_phar_entries(data)?
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a native PHAR archive and returns every decoded entry payload.
fn parse_native_phar_entries(data: &[u8]) -> Option<Vec<ArchiveEntry>> {
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
        entries.push(ArchiveEntry {
            name: name.to_vec(),
            payload,
            compression: phar_compression_from_flags(flags),
        });
        data_offset = data_offset.checked_add(compressed)?;
    }
    Some(entries)
}

/// Extracts the PHAR compression mode from per-entry flags.
fn phar_compression_from_flags(flags: u32) -> PharCompression {
    if flags & PHAR_FLAG_GZIP != 0 {
        PharCompression::Gzip
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        PharCompression::Bzip2
    } else {
        PharCompression::None
    }
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

/// Inserts `payload` under `entry_name`, preserving compression for replacements.
fn upsert_entry(entries: &mut Vec<ArchiveEntry>, entry_name: &[u8], payload: &[u8]) {
    if let Some(existing) = entries.iter_mut().find(|entry| entry.name == entry_name) {
        existing.payload.clear();
        existing.payload.extend_from_slice(payload);
    } else {
        entries.push(ArchiveEntry {
            name: entry_name.to_vec(),
            payload: payload.to_vec(),
            compression: PharCompression::None,
        });
    }
}

/// Builds a SHA1-signed native PHAR archive from decoded entries.
fn build_native_phar_archive(entries: &[ArchiveEntry]) -> Option<Vec<u8>> {
    let mut manifest = Vec::new();
    let mut stored_entries = Vec::with_capacity(entries.len());
    manifest.extend_from_slice(&u32::try_from(entries.len()).ok()?.to_le_bytes());
    manifest.extend_from_slice(&[0x11, 0x00]);
    manifest.extend_from_slice(&PHAR_HDR_SIGNATURE.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
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
        manifest.extend_from_slice(&0u32.to_le_bytes());
        stored_entries.push(stored);
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
    out.extend_from_slice(&u32::try_from(manifest.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&manifest);
    for stored in stored_entries {
        out.extend_from_slice(&stored);
    }
    append_sha1_signature(&mut out);
    Some(out)
}

/// Encodes a native PHAR payload according to its preserved compression mode.
fn encode_phar_payload(payload: &[u8], compression: PharCompression) -> Option<Vec<u8>> {
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
fn phar_compression_flag(compression: PharCompression) -> u32 {
    match compression {
        PharCompression::None => 0,
        PharCompression::Gzip => PHAR_FLAG_GZIP,
        PharCompression::Bzip2 => PHAR_FLAG_BZIP2,
    }
}

/// Builds a POSIX ustar archive with stored regular-file entries.
fn build_tar_archive(entries: &[ArchiveEntry]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for entry in entries {
        let (name, prefix) = split_tar_name(&entry.name)?;
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
        let size = format!("{:011o}\0", entry.payload.len());
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
        out.extend_from_slice(&entry.payload);
        out.resize(
            out.len() + round_up_to_512(entry.payload.len())? - entry.payload.len(),
            0,
        );
    }
    out.extend_from_slice(&[0u8; 1024]);
    Some(out)
}

/// Splits a tar entry path into ustar `name` and optional `prefix` fields.
fn split_tar_name(name: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
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

/// Builds a ZIP archive with stored entries and central-directory records.
fn build_zip_archive(entries: &[ArchiveEntry]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let name_len = u16::try_from(entry.name.len()).ok()?;
        let payload_len = u32::try_from(entry.payload.len()).ok()?;
        let local_offset = u32::try_from(out.len()).ok()?;
        let crc = crc32(&entry.payload);

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&entry.name);
        out.extend_from_slice(&entry.payload);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&ZIP_METHOD_STORE.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&payload_len.to_le_bytes());
        central.extend_from_slice(&payload_len.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(&entry.name);
    }
    let central_offset = u32::try_from(out.len()).ok()?;
    let central_len = u32::try_from(central.len()).ok()?;
    let entry_count = u16::try_from(entries.len()).ok()?;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&entry_count.to_le_bytes());
    out.extend_from_slice(&entry_count.to_le_bytes());
    out.extend_from_slice(&central_len.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
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
    parse_zip_entries(data)?
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a ZIP archive central directory and returns every supported entry.
fn parse_zip_entries(data: &[u8]) -> Option<Vec<ArchiveEntry>> {
    let eocd = find_zip_eocd(data)?;
    let entry_count = le16(data, eocd + 10)? as usize;
    let central_dir_offset = le32(data, eocd + 16)? as usize;
    let mut entries = Vec::with_capacity(entry_count);
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
        let payload = decode_zip_local_entry(
            data,
            local_offset,
            method,
            compressed_size,
            uncompressed_size,
        )?;
        entries.push(ArchiveEntry {
            name: name.to_vec(),
            payload,
            compression: PharCompression::None,
        });
        p = name_start
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(comment_len)?;
    }
    Some(entries)
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
    parse_tar_entries(data)?
        .into_iter()
        .find(|candidate| candidate.name == entry)
        .map(|candidate| candidate.payload)
}

/// Parses a POSIX tar archive and returns regular-file entries.
fn parse_tar_entries(data: &[u8]) -> Option<Vec<ArchiveEntry>> {
    let mut p = 0usize;
    let mut entries = Vec::new();
    while p.checked_add(512)? <= data.len() {
        let header = &data[p..p + 512];
        if header.iter().all(|&b| b == 0) {
            return Some(entries);
        }
        let size = parse_tar_octal(&header[124..136])?;
        let payload_start = p.checked_add(512)?;
        let payload_end = payload_start.checked_add(size)?;
        let payload = data.get(payload_start..payload_end)?;
        let typeflag = header[156];
        if typeflag == 0 || typeflag == b'0' {
            entries.push(ArchiveEntry {
                name: tar_entry_name(header)?,
                payload: payload.to_vec(),
                compression: PharCompression::None,
            });
        }
        p = payload_start.checked_add(round_up_to_512(size)?)?;
    }
    Some(entries)
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

    /// Builds a minimal native PHAR fixture with entries carrying explicit flags.
    fn build_native_phar_with_flags(entries: &[(&str, &[u8], u32, u32)]) -> Vec<u8> {
        let mut manifest = Vec::new();
        manifest.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        manifest.extend_from_slice(&[0x11, 0x00]);
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        for (name, stored, uncompressed_len, flags) in entries {
            manifest.extend_from_slice(&(name.len() as u32).to_le_bytes());
            manifest.extend_from_slice(name.as_bytes());
            manifest.extend_from_slice(&uncompressed_len.to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&(stored.len() as u32).to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
            manifest.extend_from_slice(&flags.to_le_bytes());
            manifest.extend_from_slice(&0u32.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&manifest);
        for (_, stored, _, _) in entries {
            out.extend_from_slice(stored);
        }
        out
    }

    /// Builds a minimal native PHAR fixture with uncompressed entries.
    fn build_native_phar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let entries = entries
            .iter()
            .map(|(name, content)| (*name, *content, content.len() as u32, PHAR_FILE_MODE_0644))
            .collect::<Vec<_>>();
        build_native_phar_with_flags(&entries)
    }

    /// Builds a raw-DEFLATE payload for PHAR gzip entry fixtures.
    fn deflate_payload(content: &[u8]) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
    }

    /// Builds a bzip2 payload for PHAR bzip2 entry fixtures.
    fn bzip2_payload(content: &[u8]) -> Vec<u8> {
        let mut encoder =
            bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
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
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"alpha"),
            Some(5)
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"one.txt", b"updated"),
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

    /// Verifies native PHAR writes preserve gzip compression on replaced entries.
    #[test]
    fn writes_preserve_gzip_native_phar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_gzip_update_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let original = b"gzip old payload gzip old payload";
        let stored = deflate_payload(original);
        let archive = build_native_phar_with_flags(&[(
            "z.txt",
            &stored,
            original.len() as u32,
            PHAR_FILE_MODE_0644 | PHAR_FLAG_GZIP,
        )]);
        std::fs::write(&path, archive).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"z.txt", b"gzip updated payload"),
            Some(20)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let entries = parse_native_phar_entries(&archive).unwrap();
        assert_eq!(entries[0].compression, PharCompression::Gzip);
        assert_eq!(entries[0].payload, b"gzip updated payload");
    }

    /// Verifies native PHAR writes preserve bzip2 compression on replaced entries.
    #[test]
    fn writes_preserve_bzip2_native_phar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_bzip2_update_{}_{}.phar",
            std::process::id(),
            "unit"
        ));
        let original = b"bzip2 old payload bzip2 old payload";
        let stored = bzip2_payload(original);
        let archive = build_native_phar_with_flags(&[(
            "b.txt",
            &stored,
            original.len() as u32,
            PHAR_FILE_MODE_0644 | PHAR_FLAG_BZIP2,
        )]);
        std::fs::write(&path, archive).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"b.txt", b"bzip2 updated payload"),
            Some(21)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let entries = parse_native_phar_entries(&archive).unwrap();
        assert_eq!(entries[0].compression, PharCompression::Bzip2);
        assert_eq!(entries[0].payload, b"bzip2 updated payload");
    }

    /// Verifies tar writes preserve the tar container family while updating entries.
    #[test]
    fn writes_tar_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_tar_write_{}_{}.tar",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&path, build_tar(&[("one.txt", b"alpha")])).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
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
        assert_ne!(archive.get(0..5), Some(&b"<?php"[..]));
    }

    /// Verifies ZIP writes preserve the ZIP container family while updating entries.
    #[test]
    fn writes_zip_entries() {
        let path = std::env::temp_dir().join(format!(
            "elephc_phar_zip_write_{}_{}.zip",
            std::process::id(),
            "unit"
        ));
        std::fs::write(&path, build_zip(&[("one.txt", b"alpha", true)])).unwrap();
        let path_bytes = path.to_string_lossy();
        assert_eq!(
            put_entry_bytes(path_bytes.as_bytes(), b"dir/two.txt", b"bravo"),
            Some(5)
        );
        let archive = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(archive.get(0..4), Some(&[0x50, 0x4b, 0x03, 0x04][..]));
        assert_eq!(
            extract_entry_bytes(&archive, b"one.txt").as_deref(),
            Some(&b"alpha"[..])
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
