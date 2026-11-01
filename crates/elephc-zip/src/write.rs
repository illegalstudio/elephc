//! Purpose:
//! Serializes an `Archive` back to disk with libzip-compatible entry preservation.
//!
//! Called from:
//! - `crate::abi::elephc_zip_close` through `Archive::close`.
//!
//! Key details:
//! - Untouched original entries are raw-copied, preserving unknown methods/encryption.
//! - Mutated entries are re-emitted from their decompressed payload with exact mtimes.

use std::fs::{self, File};
use std::io::{BufWriter, Cursor, Write};
use std::path::Path;

use zip::write::{FullFileOptions, SimpleFileOptions};
use zip::{AesMode, DateTime, System, ZipArchive, ZipWriter};

use crate::consts::*;
use crate::error::ZipError;
use crate::model::{Archive, Entry, Source};

/// Writes the archive to its path, returning libzip's success or failure.
pub(crate) fn save(archive: &mut Archive) -> Result<(), ZipError> {
    let path = Path::new(&archive.path);
    let live = archive.entries.iter().filter(|entry| !entry.deleted).count();
    if live == 0 && archive.archive_flags & AFL_KEEP_EMPTY == 0 {
        if path.exists() {
            fs::remove_file(path).map_err(|_| ZipError::Remove)?;
        }
        return Ok(());
    }
    let temporary = temporary_path(path);
    let result = write_to(archive, &temporary);
    match result {
        Ok(()) => {
            fs::rename(&temporary, path).map_err(|_| ZipError::Rename)?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// Returns a sibling temporary path so the final rename stays atomic.
fn temporary_path(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.elephc-zip-{}", std::process::id()))
}

/// Writes the complete archive into `target`.
fn write_to(archive: &Archive, target: &Path) -> Result<(), ZipError> {
    let file = File::create(target).map_err(|_| ZipError::TempOpen)?;
    let mut writer = ZipWriter::new(BufWriter::new(file));
    let torrentzip = archive.archive_flags & AFL_WANT_TORRENTZIP != 0;
    let comment = if torrentzip {
        torrent_comment(archive)
    } else {
        archive.comment.clone()
    };
    writer.set_raw_comment(comment.into_boxed_slice()).map_err(ZipError::from)?;

    let mut source = ZipArchive::new(Cursor::new(archive.original_bytes.clone())).ok();
    for entry in archive.entries.iter().filter(|entry| !entry.deleted) {
        match &entry.source {
            Source::Original(index) if !entry.dirty => {
                // Never silently drop an untouched entry because the second parser rejected bytes
                // our parser accepted: a successful close must preserve every requested entry.
                let source = source.as_mut().ok_or(ZipError::Read)?;
                let file = source.by_index_raw(*index).map_err(ZipError::from)?;
                if entry.renamed {
                    writer.raw_copy_file_rename(file, String::from_utf8_lossy(&entry.name).into_owned())
                        .map_err(ZipError::from)?;
                } else {
                    writer.raw_copy_file(file).map_err(ZipError::from)?;
                }
            }
            _ => write_entry(&mut writer, archive, entry)?,
        }
    }
    if torrentzip {
        writer.set_raw_comment(torrent_comment(archive).into_boxed_slice()).map_err(ZipError::from)?;
    }
    let inner = writer.finish().map_err(ZipError::from)?;
    let mut inner = inner;
    inner.flush().map_err(ZipError::from)?;
    drop(inner);
    let zipcrypto: Vec<Vec<u8>> = archive.entries.iter()
        .filter(|entry| !entry.deleted && entry.encryption == EM_TRAD_PKWARE)
        .map(|entry| entry.name.clone())
        .collect();
    if !zipcrypto.is_empty() {
        let mut bytes = std::fs::read(target).map_err(ZipError::from)?;
        crate::metadata::patch_encrypted_flags(&mut bytes, &zipcrypto)?;
        std::fs::write(target, bytes).map_err(ZipError::from)?;
    }
    Ok(())
}

/// Builds the deterministic torrentzip marker comment.
fn torrent_comment(archive: &Archive) -> Vec<u8> {
    let mut crc = crc32fast::Hasher::new();
    for entry in archive.entries.iter().filter(|entry| !entry.deleted) {
        crc.update(&entry.name);
        let data = entry.data.as_deref().unwrap_or_default();
        crc.update(data);
    }
    format!("TORRENTZIPPED-{:08X}", crc.finalize()).into_bytes()
}

/// Chooses the compression method written for one entry.
///
/// Pending memory entries are reported as CM_STORE until close, but libzip stores them as
/// CM_DEFLATE when deflate actually shrinks the payload. Original entries keep their method.
fn output_method(entry: &Entry, data: &[u8]) -> i64 {
    if entry.is_dir() {
        return CM_STORE;
    }
    if entry.method_explicit {
        return entry.method;
    }
    if matches!(&entry.source, Source::Memory) {
        let (compressed_size, _) = crate::compress::deflate_stats(data);
        return if compressed_size < data.len() as u64 { CM_DEFLATE } else { CM_STORE };
    }
    entry.method
}

/// Writes one memory-backed or metadata-changed entry.
fn write_entry<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    archive: &Archive,
    entry: &Entry,
) -> Result<(), ZipError> {
    if entry.encryption == EM_TRAD_PKWARE {
        return write_zipcrypto_entry(writer, archive, entry);
    }
    let data = match &entry.source {
        Source::Memory => entry.data.clone().ok_or(ZipError::Internal)?,
        // Header-only rewrites must preserve entries whose CRC is wrong or whose
        // decompressor reports an error after yielding bytes. PHP/libzip keeps the
        // payload here; the stream reader is the only place that surfaces CRC errors.
        Source::Original(index) => archive.payload_stream(*index, FL_UNCHANGED, None)?.0,
    };
    let method = output_method(entry, &data);
    let engine = crate::compress::engine_method(method).ok_or(ZipError::CompNotSupported)?;
    let mut options: FullFileOptions = SimpleFileOptions::default()
        .compression_method(engine)
        .last_modified_time(dos_time(entry.mtime))
        .external_attributes(entry.external_attrs)
        .system(system(entry.system))
        .into_full_options();
    options.add_extra_field(0x5455, extended_timestamp(entry.mtime), false).map_err(ZipError::from)?;
    if !entry.comment.is_empty() {
        options = options.with_file_comment(String::from_utf8_lossy(&entry.comment).into_owned());
    }
    let password = entry.password.as_deref().or(archive.password.as_deref());
    if entry.encryption != EM_NONE {
        let Some(password) = password else { return Err(ZipError::NoPassword) };
        let mode = match entry.encryption {
            EM_AES_128 => AesMode::Aes128,
            EM_AES_192 => AesMode::Aes192,
            _ => AesMode::Aes256,
        };
        options = options.with_aes_encryption_bytes(mode, password);
    }
    writer.start_file(String::from_utf8_lossy(&entry.name).into_owned(), options).map_err(ZipError::from)?;
    if !entry.is_dir() {
        writer.write_all(&data).map_err(ZipError::from)?;
    }
    Ok(())
}

/// Writes one traditional-PKWARE encrypted entry through a single-entry template.
///
/// The zip crate does not expose ZipCrypto writing, so this builds a correct one-entry
/// archive with our own local/central headers, parses it back, and raw-copies the encrypted
/// payload into the real writer. Raw copying preserves the encrypted flag and compressed bytes.
fn write_zipcrypto_entry<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    archive: &Archive,
    entry: &Entry,
) -> Result<(), ZipError> {
    let data = match &entry.source {
        Source::Memory => entry.data.clone().ok_or(ZipError::Internal)?,
        Source::Original(index) => archive.payload_stream(*index, FL_UNCHANGED, None)?.0,
    };
    let method_id = match output_method(entry, &data) {
        CM_STORE => 0u16,
        CM_DEFAULT | CM_DEFLATE => 8u16,
        _ => return Err(ZipError::CompNotSupported),
    };
    let compressed = if method_id == 0 {
        data.clone()
    } else {
        crate::compress::deflate(&data)
    };
    let crc = crate::compress::crc32(&data);
    let password = entry.password.as_deref().or(archive.password.as_deref())
        .ok_or(ZipError::NoPassword)?;
    let payload = zipcrypto_encrypt(password, &compressed, (crc >> 24) as u8);
    let name = &entry.name;
    let name_len = u16::try_from(name.len()).map_err(|_| ZipError::Invalid)?;
    let timestamp = extended_timestamp(entry.mtime);
    let mut extra = Vec::with_capacity(timestamp.len() + 4);
    extra.extend_from_slice(&0x5455u16.to_le_bytes());
    extra.extend_from_slice(&(timestamp.len() as u16).to_le_bytes());
    extra.extend_from_slice(&timestamp);
    let extra_len = u16::try_from(extra.len()).map_err(|_| ZipError::Invalid)?;
    let comment = &entry.comment;
    let comment_len = u16::try_from(comment.len()).map_err(|_| ZipError::Invalid)?;
    let (time, date) = dos_fields(entry.mtime);
    let flags = entry.flags | 0x0801;
    let version = 20u16;
    let system = entry.system.clamp(0, 255) as u16;
    let made_by = (system << 8) | version;

    let mut out = Vec::new();
    // Local file header at offset 0.
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&method_id.to_le_bytes());
    out.extend_from_slice(&time.to_le_bytes());
    out.extend_from_slice(&date.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&extra_len.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&extra);
    out.extend_from_slice(&payload);
    let central_offset = out.len() as u32;
    // Central directory record.
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&made_by.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&method_id.to_le_bytes());
    out.extend_from_slice(&time.to_le_bytes());
    out.extend_from_slice(&date.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&extra_len.to_le_bytes());
    out.extend_from_slice(&comment_len.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&entry.external_attrs.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&extra);
    out.extend_from_slice(comment);
    let central_size = out.len() as u32 - central_offset;
    // End of central directory.
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());

    let mut template = ZipArchive::new(Cursor::new(out)).map_err(ZipError::from)?;
    let file = template.by_index_raw(0).map_err(ZipError::from)?;
    writer.raw_copy_file_rename(file, String::from_utf8_lossy(name).into_owned()).map_err(ZipError::from)?;
    Ok(())
}

/// Traditional PKWARE cipher state, matching PKWARE's three-key schedule.
struct ZipCryptoKeys { k0: u32, k1: u32, k2: u32 }

impl ZipCryptoKeys {
    /// Seeds the three keys from the password bytes.
    fn new(password: &[u8]) -> Self {
        let mut keys = Self { k0: 0x1234_5678, k1: 0x2345_6789, k2: 0x3456_7890 };
        for &byte in password { keys.update(byte); }
        keys
    }

    /// Advances the three keys with one plaintext byte.
    fn update(&mut self, byte: u8) {
        self.k0 = crc32_byte(self.k0, byte);
        self.k1 = self.k1.wrapping_add(self.k0 & 0xff);
        self.k1 = self.k1.wrapping_mul(134_775_813).wrapping_add(1);
        self.k2 = crc32_byte(self.k2, (self.k1 >> 24) as u8);
    }

    /// Returns the next keystream byte derived from `k2`.
    fn keystream(&self) -> u8 {
        let temp = (self.k2 | 2) & 0xffff;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    /// Encrypts one plaintext byte and advances the keys.
    fn encrypt(&mut self, plain: u8) -> u8 {
        let cipher = plain ^ self.keystream();
        self.update(plain);
        cipher
    }
}

/// One-byte CRC32 step used by the ZipCrypto key schedule.
fn crc32_byte(crc: u32, byte: u8) -> u32 {
    let mut t = (crc ^ byte as u32) & 0xff;
    for _ in 0..8 {
        t = if t & 1 != 0 { (t >> 1) ^ 0xedb8_8320 } else { t >> 1 };
    }
    (crc >> 8) ^ t
}

/// Encrypts a compressed payload with a 12-byte ZipCrypto header and check byte.
fn zipcrypto_encrypt(password: &[u8], data: &[u8], check_byte: u8) -> Vec<u8> {
    let mut header = [0u8; 12];
    for (index, byte) in header[..11].iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(37).wrapping_add((std::process::id() as u8).wrapping_mul(11));
    }
    header[11] = check_byte;
    let mut keys = ZipCryptoKeys::new(password);
    let mut out = Vec::with_capacity(data.len() + 12);
    for &plain in header.iter().chain(data) { out.push(keys.encrypt(plain)); }
    out
}

/// Returns the DOS date/time fields for a Unix timestamp.
fn dos_fields(timestamp: i64) -> (u16, u16) {
    let (year, month, day, hour, minute, second) = crate::timeutil::unix_to_civil(timestamp);
    let date = (((year.clamp(1980, 2107) - 1980) as u16) << 9)
        | ((month.clamp(1, 12) as u16) << 5)
        | day.clamp(1, 31) as u16;
    let time = ((hour.clamp(0, 23) as u16) << 11)
        | ((minute.clamp(0, 59) as u16) << 5)
        | ((second.clamp(0, 58) / 2) as u16);
    (time, date)
}

/// Builds the extended-timestamp extra body carrying a Unix mtime.
fn extended_timestamp(timestamp: i64) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);
    body.push(1);
    body.extend_from_slice(&(timestamp as u32).to_le_bytes());
    body
}

/// Converts a Unix timestamp into a ZIP DOS date/time.
fn dos_time(timestamp: i64) -> DateTime {
    let (year, month, day, hour, minute, second) = crate::timeutil::unix_to_civil(timestamp);
    DateTime::from_date_and_time(
        year.clamp(1980, 2107) as u16,
        month.clamp(1, 12) as u8,
        day.clamp(1, 31) as u8,
        hour.clamp(0, 23) as u8,
        minute.clamp(0, 59) as u8,
        second.clamp(0, 58) as u8,
    ).unwrap_or(DateTime::default())
}

/// Maps a libzip `OPSYS_*` value to the writer's host-system enum.
fn system(opsys: i64) -> System {
    match opsys {
        0 => System::Dos,
        1 => System::Amiga,
        2 => System::OpenVMS,
        3 => System::Unix,
        4 => System::VmCms,
        5 => System::AtariSt,
        6 => System::Os2,
        7 => System::Macintosh,
        8 => System::ZSystem,
        9 => System::CPM,
        10 => System::WindowsNTFS,
        11 => System::MVS,
        12 => System::VSE,
        13 => System::AcornRisc,
        14 => System::VFAT,
        15 => System::AlternateMVS,
        16 => System::BeOS,
        17 => System::Tandem,
        18 => System::Os400,
        19 => System::OsDarwin,
        _ => System::Unix,
    }
}
