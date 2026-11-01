//! Purpose:
//! Owns the mutable ZIP archive model: entries, comments, flags, passwords, and libzip
//! status reporting.
//!
//! Called from:
//! - `crate::abi` for every PHP-visible `ZipArchive` operation.
//!
//! Key details:
//! - Indexes are stable: deleted entries stay in place until close, matching libzip.
//! - Original bytes are retained so untouched entries can be raw-copied on rewrite.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::consts::*;
use crate::error::ZipError;
use crate::metadata::{self, Directory, RawEntry};

/// How an entry's payload is obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// Bytes stored inside the archive loaded at open time.
    Original(usize),
    /// Bytes owned by this model (a new or replaced entry).
    Memory,
}

/// One logical archive entry, including deleted placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// UTF-8 name PHP sees by default.
    pub(crate) name: Vec<u8>,
    /// Stored name bytes (raw CP437 or UTF-8 as in the archive).
    pub(crate) name_raw: Vec<u8>,
    /// General-purpose flags as stored.
    pub(crate) flags: u16,
    /// Current libzip compression method.
    pub(crate) method: i64,
    /// Whether `setCompression*()` chose the method explicitly. Pending memory entries
    /// otherwise report CM_STORE until the writer picks store/deflate from the payload.
    pub(crate) method_explicit: bool,
    /// Current uncompressed size.
    pub(crate) size: u64,
    /// Current compressed size.
    pub(crate) comp_size: u64,
    /// Current CRC-32.
    pub(crate) crc: u32,
    /// Unix modification timestamp.
    pub(crate) mtime: i64,
    /// Entry comment bytes.
    pub(crate) comment: Vec<u8>,
    /// External attributes.
    pub(crate) external_attrs: u32,
    /// Host system byte for external attributes.
    pub(crate) system: i64,
    /// libzip encryption method.
    pub(crate) encryption: i64,
    /// Per-entry encryption password.
    pub(crate) password: Option<Vec<u8>>,
    /// Payload source.
    pub(crate) source: Source,
    /// Decompressed payload for memory-backed entries.
    pub(crate) data: Option<Vec<u8>>,
    /// Filesystem path this entry was added from, when close must re-check it.
    pub(crate) source_path: Option<String>,
    /// Deleted placeholder; still counts toward `numFiles` until close.
    pub(crate) deleted: bool,
    /// Whether data or metadata differ from the original snapshot.
    pub(crate) dirty: bool,
    /// Whether the name differs from the original snapshot.
    pub(crate) renamed: bool,
}

impl Entry {
    /// Builds a memory-backed entry from a name and payload.
    pub(crate) fn memory(name: Vec<u8>, data: Vec<u8>, mtime: i64) -> Self {
        // Pending entries expose PHP/libzip's "not compressed yet" stat shape: CM_STORE,
        // comp_size == size and crc == 0. The writer chooses CM_STORE vs CM_DEFLATE at
        // close once it knows whether compression actually shrinks the payload.
        let method = CM_STORE;
        let size = data.len() as u64;
        Self {
            name_raw: name.clone(),
            name,
            flags: 0x0800,
            method,
            method_explicit: false,
            size,
            comp_size: size,
            crc: 0,
            mtime,
            comment: Vec::new(),
            external_attrs: 0o100644 << 16,
            system: 3,
            encryption: EM_NONE,
            password: None,
            source: Source::Memory,
            data: Some(data),
            source_path: None,
            deleted: false,
            dirty: true,
            renamed: false,
        }
    }

    /// Returns whether this entry represents a directory.
    pub(crate) fn is_dir(&self) -> bool {
        self.name.ends_with(b"/")
    }

    /// Recomputes size, CRC, and compressed size after a payload change.
    pub(crate) fn refresh_payload(&mut self, data: Vec<u8>) {
        self.size = data.len() as u64;
        self.crc = crate::compress::crc32(&data);
        self.comp_size = crate::compress::stored_size(self.method, &data);
        self.data = Some(data);
        self.source = Source::Memory;
        self.dirty = true;
    }
}

/// One open archive.
pub(crate) struct Archive {
    /// Filename as supplied by PHP.
    pub(crate) path: String,
    /// Current entries in stable index order.
    pub(crate) entries: Vec<Entry>,
    /// Snapshot of entries at open time.
    pub(crate) original: Vec<Entry>,
    /// Current archive comment.
    pub(crate) comment: Vec<u8>,
    /// Archive comment at open time.
    pub(crate) original_comment: Vec<u8>,
    /// Original archive bytes for raw copies.
    pub(crate) original_bytes: Vec<u8>,
    /// Last libzip error.
    pub(crate) status: ZipError,
    /// Last OS errno (always 0 in this engine).
    pub(crate) status_sys: i64,
    /// Index of the last entry added or replaced.
    pub(crate) last_id: i64,
    /// Default password set by `setPassword`.
    pub(crate) password: Option<Vec<u8>>,
    /// `AFL_*` archive flags.
    pub(crate) archive_flags: i64,
    /// Whether the archive was opened read-only.
    pub(crate) readonly: bool,
    /// Whether `close` has already run.
    pub(crate) opened: bool,
    /// Whether any mutation requires a rewrite.
    pub(crate) dirty: bool,
    /// Whether an existing zero-byte file was opened as an empty archive.
    pub(crate) empty_file: bool,
    /// Whether open created or truncated the backing file.
    pub(crate) created: bool,
}

impl Archive {
    /// Opens or creates one archive, matching libzip's flag matrix.
    pub(crate) fn open(path: String, flags: i64) -> Result<Self, ZipError> {
        let file = PathBuf::from(&path);
        let exists = file.is_file();
        let readonly = flags & RDONLY != 0;
        let create = flags & CREATE != 0;
        let overwrite = flags & OVERWRITE != 0;
        if file.exists() && !exists {
            return Err(ZipError::NotSupported);
        }
        if exists && flags & EXCL != 0 && create {
            return Err(ZipError::Exists);
        }
        let mut archive = Self {
            path: path.clone(),
            entries: Vec::new(),
            original: Vec::new(),
            comment: Vec::new(),
            original_comment: Vec::new(),
            original_bytes: Vec::new(),
            status: ZipError::Ok,
            status_sys: 0,
            last_id: -1,
            password: None,
            archive_flags: if readonly { AFL_RDONLY } else { 0 },
            readonly,
            opened: true,
            dirty: false,
            empty_file: false,
            created: false,
        };
        if exists && !overwrite {
            let bytes = std::fs::read(&file).map_err(ZipError::from)?;
            if bytes.is_empty() {
                if !create {
                    archive.empty_file = true;
                    return Ok(archive);
                }
                return Ok(archive);
            }
            let directory = metadata::parse(&bytes)?;
            if flags & CHECKCONS != 0 {
                metadata::check_consistency(&bytes, &directory)?;
            }
            archive.load_directory(directory, bytes);
            return Ok(archive);
        }
        if !exists && !create {
            return Err(ZipError::NoEnt);
        }
        // CREATE/OVERWRITE starts empty but defers touching the filesystem until close,
        // matching PHP: an OVERWRITE keeps the original file intact until the new archive is
        // committed, and a CREATE that never adds an entry leaves no file behind.
        archive.created = true;
        archive.dirty = false;
        Ok(archive)
    }

    /// Populates current and original entries from a parsed directory.
    fn load_directory(&mut self, directory: Directory, bytes: Vec<u8>) {
        self.original_bytes = bytes;
        self.comment = directory.comment.clone();
        self.original_comment = directory.comment;
        if directory.torrentzip {
            self.archive_flags |= AFL_IS_TORRENTZIP;
        }
        self.entries = directory.entries.iter().enumerate().map(|(index, raw)| entry_from_raw(raw, index)).collect();
        self.original = self.entries.clone();
    }

    /// Records one successful call's status.
    pub(crate) fn success(&mut self) {
        self.status = ZipError::Ok;
        self.status_sys = 0;
    }

    /// Records one failed call's status.
    pub(crate) fn fail(&mut self, error: ZipError) -> i64 {
        self.status = error;
        self.status_sys = 0;
        error.code()
    }

    /// Returns whether mutations are allowed.
    fn writable(&mut self) -> Result<(), ZipError> {
        if self.readonly || self.archive_flags & AFL_RDONLY != 0 {
            return Err(ZipError::ReadOnly);
        }
        if !self.opened {
            return Err(ZipError::ZipClosed);
        }
        Ok(())
    }

    /// Locates one entry index by name and lookup flags.
    pub(crate) fn locate(&self, name: &[u8], flags: i64) -> Option<usize> {
        let unchanged = flags & FL_UNCHANGED != 0;
        let entries = if unchanged { &self.original } else { &self.entries };
        entries.iter().enumerate().find(|(_, entry)| {
            if entry.name.is_empty() && !unchanged {
                return false;
            }
            if !unchanged && entry.deleted {
                return false;
            }
            let candidate = if flags & FL_NODIR != 0 { basename(&entry.name) } else { &entry.name };
            if flags & FL_NOCASE != 0 {
                candidate.eq_ignore_ascii_case(name)
            } else {
                candidate == name
            }
        }).map(|(index, _)| index)
    }

    /// Reads one entry's decompressed payload, honoring passwords.
    pub(crate) fn payload(&self, index: usize, flags: i64, password: Option<&[u8]>) -> Result<Vec<u8>, ZipError> {
        // `getFrom*()` and `extractTo()` deliver bytes even when the stored CRC is wrong:
        // libzip reports CRC failures while streaming, not while materializing a one-shot
        // payload. `payload_stream()` carries that same ignore-CRC contract.
        self.payload_stream(index, flags, password).map(|(data, _crc_error)| data)
    }

    /// Reads one entry's payload for stream wrappers, ignoring CRC mismatches.
    ///
    /// Returns `(bytes, crc_error)`: data is still delivered when the stored CRC does not
    /// match, matching libzip's `getStream()` behavior where the error surfaces on read.
    pub(crate) fn payload_stream(
        &self, index: usize, flags: i64, password: Option<&[u8]>,
    ) -> Result<(Vec<u8>, bool), ZipError> {
        let unchanged = flags & FL_UNCHANGED != 0;
        let entry = if unchanged {
            self.original.get(index).ok_or(ZipError::NoEnt)?
        } else {
            self.entries.get(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?
        };
        if let Some(data) = &entry.data {
            // Pending memory entries carry crc = 0 until close; their writer computes the
            // final CRC, so they must never be reported as a CRC-corrupt stream.
            return Ok((data.clone(), false));
        }
        let Source::Original(original_index) = entry.source else {
            return Err(ZipError::Internal);
        };
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(self.original_bytes.clone()))
            .map_err(ZipError::from)?;
        let effective = password.or(entry.password.as_deref()).or(self.password.as_deref());
        let mut options = zip::read::ZipReadOptions::new().ignore_crc32(true);
        if let Some(password) = effective {
            options = options.password(Some(password));
        } else if entry.encryption != EM_NONE {
            return Err(ZipError::NoPassword);
        }
        let opened = archive.by_index_with_options(original_index, options);
        let mut data = Vec::new();
        match opened {
            Ok(mut file) => {
                if let Err(error) = file.read_to_end(&mut data) {
                    if !is_size_limit_error(&error) {
                        return Err(ZipError::from(error));
                    }
                    data = self.raw_entry_payload(original_index, entry)?;
                }
            }
            Err(error) => {
                let size_limit = matches!(&error, zip::result::ZipError::Io(io) if is_size_limit_error(io));
                if !size_limit {
                    return Err(ZipError::from(error));
                }
                data = self.raw_entry_payload(original_index, entry)?;
            }
        }
        let crc_error = crate::compress::crc32(&data) != entry.crc;
        Ok((data, crc_error))
    }

    /// Returns one entry's compressed payload for `FL_COMPRESSED` reads.
    pub(crate) fn compressed_payload(&self, index: usize, flags: i64) -> Result<Vec<u8>, ZipError> {
        let unchanged = flags & FL_UNCHANGED != 0;
        let entry = if unchanged { self.original.get(index) } else { self.entries.get(index) }
            .filter(|entry| !entry.deleted)
            .ok_or(ZipError::NoEnt)?;
        if let Source::Original(original_index) = entry.source {
            let raw = self.original_entry(original_index).ok_or(ZipError::Internal)?;
            if entry.encryption != EM_NONE && flags & FL_ENCRYPTED == 0 {
                // libzip still returns compressed bytes; callers that need decryption use payload().
            }
            let start = metadata::local_payload_offset(&self.original_bytes, &raw).ok_or(ZipError::Truncated)?;
            let end = start.checked_add(raw.comp_size as usize).ok_or(ZipError::Truncated)?;
            return self.original_bytes.get(start..end).map(|bytes| bytes.to_vec()).ok_or(ZipError::Truncated);
        }
        let data = entry.data.as_deref().unwrap_or_default();
        Ok(crate::compress::compress(entry.method, data))
    }

    /// Decompresses an unencrypted stored/deflated entry without enforcing the declared size.
    fn raw_entry_payload(&self, index: usize, entry: &Entry) -> Result<Vec<u8>, ZipError> {
        let raw = self.original_entry(index).ok_or(ZipError::Internal)?;
        let start = metadata::local_payload_offset(&self.original_bytes, &raw).ok_or(ZipError::Truncated)?;
        let end = start.checked_add(raw.comp_size as usize).ok_or(ZipError::Truncated)?;
        let bytes = self.original_bytes.get(start..end).ok_or(ZipError::Truncated)?;
        match entry.method {
            CM_STORE => Ok(bytes.to_vec()),
            CM_DEFLATE | CM_DEFAULT => {
                let mut decoder = flate2::read::DeflateDecoder::new(bytes);
                let mut data = Vec::new();
                decoder.read_to_end(&mut data).map_err(ZipError::from)?;
                Ok(data)
            }
            _ => Err(ZipError::CompNotSupported),
        }
    }

    /// Returns the raw central-directory record behind an original index.
    fn original_entry(&self, index: usize) -> Option<RawEntry> {
        metadata::parse(&self.original_bytes).ok()?.entries.get(index).cloned()
    }

    /// Counts live entries in the `numFiles` view.
    pub(crate) fn num_entries(&self) -> usize {
        self.entries.len()
    }

    /// Appends a new memory-backed entry, or replaces one with `FL_OVERWRITE`.
    pub(crate) fn add_from_string(&mut self, name: &[u8], data: Vec<u8>, flags: i64) -> Result<usize, ZipError> {
        self.writable()?;
        let name = input_name(name, flags);
        let now = crate::clock::now_unix();
        let entry = Entry::memory(name, data, now);
        let name = entry.name.clone();
        self.insert_entry(entry, &name, flags)
    }

    /// Appends a memory entry that remembers its source path for close-time checks.
    pub(crate) fn add_from_path(&mut self, name: &[u8], data: Vec<u8>, flags: i64, source_path: Option<String>) -> Result<usize, ZipError> {
        let index = self.add_from_string(name, data, flags)?;
        if let Some(entry) = self.entries.get_mut(index) {
            entry.source_path = source_path;
        }
        Ok(index)
    }

    /// Appends an empty directory entry.
    pub(crate) fn add_directory(&mut self, name: &[u8], flags: i64) -> Result<usize, ZipError> {
        self.writable()?;
        let mut full = input_name(name, flags);
        if !full.ends_with(b"/") { full.push(b'/'); }
        let mut entry = Entry::memory(full.clone(), Vec::new(), crate::clock::now_unix());
        entry.method = CM_STORE;
        entry.comp_size = 0;
        entry.crc = 0;
        entry.external_attrs = 0o040755 << 16;
        self.insert_entry(entry, &full, flags)
    }

    /// Inserts one entry, preserving the replaced index for overwrite adds.
    fn insert_entry(&mut self, entry: Entry, name: &[u8], flags: i64) -> Result<usize, ZipError> {
        if let Some(index) = self.locate(name, 0) {
            if flags & FL_OVERWRITE == 0 {
                self.last_id = -1;
                return Err(ZipError::Exists);
            }
            let existing = &mut self.entries[index];
            let keep_name = existing.name.clone();
            *existing = entry;
            existing.name = keep_name.clone();
            existing.name_raw = keep_name;
            existing.dirty = true;
            self.last_id = index as i64;
            self.dirty = true;
            self.success();
            return Ok(index);
        }
        let index = self.entries.len();
        self.entries.push(entry);
        self.last_id = index as i64;
        self.dirty = true;
        self.success();
        Ok(index)
    }

    /// Replaces one existing entry's payload.
    pub(crate) fn replace_entry(&mut self, index: usize, data: Vec<u8>) -> Result<usize, ZipError> {
        self.writable()?;
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.refresh_payload(data);
        self.dirty = true;
        self.last_id = index as i64;
        self.success();
        Ok(index)
    }

    /// Marks one entry deleted.
    pub(crate) fn delete(&mut self, index: usize) -> Result<(), ZipError> {
        self.writable()?;
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::Invalid)?;
        entry.deleted = true;
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Renames one entry.
    pub(crate) fn rename(&mut self, index: usize, name: &[u8]) -> Result<(), ZipError> {
        self.writable()?;
        if self.locate(name, 0).is_some_and(|existing| existing != index) {
            return Err(ZipError::Exists);
        }
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.name = name.to_vec();
        entry.name_raw = name.to_vec();
        entry.flags |= 0x0800;
        entry.renamed = true;
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Sets one entry's comment.
    pub(crate) fn set_comment(&mut self, index: usize, comment: &[u8]) -> Result<(), ZipError> {
        self.writable()?;
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.comment = comment.to_vec();
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Sets one entry's mtime.
    pub(crate) fn set_mtime(&mut self, index: usize, timestamp: i64) -> Result<(), ZipError> {
        self.writable()?;
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.mtime = timestamp;
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Sets one entry's compression method.
    pub(crate) fn set_compression(&mut self, index: usize, method: i64) -> Result<(), ZipError> {
        self.writable()?;
        if !crate::compress::writable_method(method) {
            return Err(ZipError::CompNotSupported);
        }
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.method = method;
        entry.method_explicit = method != CM_DEFAULT;
        let data = entry.data.clone();
        if let Some(data) = data {
            entry.comp_size = crate::compress::stored_size(method, &data);
        }
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Sets one entry's encryption method and password.
    pub(crate) fn set_encryption(&mut self, index: usize, method: i64, password: Option<Vec<u8>>) -> Result<(), ZipError> {
        self.writable()?;
        if !matches!(method, EM_NONE | EM_TRAD_PKWARE | EM_AES_128 | EM_AES_192 | EM_AES_256) {
            return Err(ZipError::EncryptNotSupported);
        }
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.encryption = method;
        entry.password = password;
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Sets one entry's external attributes.
    pub(crate) fn set_external_attributes(&mut self, index: usize, opsys: i64, attr: u32) -> Result<(), ZipError> {
        self.writable()?;
        let entry = self.entries.get_mut(index).filter(|entry| !entry.deleted).ok_or(ZipError::NoEnt)?;
        entry.system = opsys;
        entry.external_attrs = attr;
        entry.dirty = true;
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Reads one entry's external attributes with `FL_UNCHANGED` support.
    pub(crate) fn external_attributes(&self, index: usize, flags: i64) -> Result<(i64, u32), ZipError> {
        let entry = if flags & FL_UNCHANGED != 0 { self.original.get(index) } else { self.entries.get(index) }
            .filter(|entry| !entry.deleted)
            .ok_or(ZipError::NoEnt)?;
        Ok((entry.system, entry.external_attrs))
    }

    /// Sets the archive comment.
    pub(crate) fn set_archive_comment(&mut self, comment: &[u8]) -> Result<(), ZipError> {
        self.writable()?;
        self.comment = comment.to_vec();
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Sets one archive-wide flag.
    pub(crate) fn set_archive_flag(&mut self, flag: i64, value: i64) -> Result<(), ZipError> {
        if self.readonly || self.archive_flags & AFL_RDONLY != 0 {
            return Err(ZipError::ReadOnly);
        }
        match flag {
            AFL_RDONLY => {
                if value != 0 { self.archive_flags |= AFL_RDONLY; } else { self.archive_flags &= !AFL_RDONLY; }
            }
            AFL_WANT_TORRENTZIP => {
                if value != 0 { self.archive_flags |= AFL_WANT_TORRENTZIP; } else { self.archive_flags &= !AFL_WANT_TORRENTZIP; }
            }
            AFL_KEEP_EMPTY => {
                if value != 0 { self.archive_flags |= AFL_KEEP_EMPTY; } else { self.archive_flags &= !AFL_KEEP_EMPTY; }
            }
            _ => return Err(ZipError::Invalid),
        }
        self.dirty = true;
        self.success();
        Ok(())
    }

    /// Closes the archive, rewriting it when needed.
    pub(crate) fn close(&mut self) -> Result<(), ZipError> {
        if !self.opened {
            return Err(ZipError::Invalid);
        }
        if self.readonly || self.archive_flags & AFL_RDONLY != 0 {
            self.opened = false;
            self.success();
            return Ok(());
        }
        for entry in self.entries.iter().filter(|entry| !entry.deleted) {
            if let Some(path) = &entry.source_path {
                if !Path::new(path).is_file() {
                    return Err(ZipError::Open);
                }
            }
        }
        let empty = self.entries.iter().all(|entry| entry.deleted);
        let remove_empty = empty && self.archive_flags & AFL_KEEP_EMPTY == 0;
        if self.dirty || self.created || remove_empty {
            crate::write::save(self)?;
        }
        self.opened = false;
        self.success();
        Ok(())
    }

    /// Recomputes whether the model still differs from the open-time snapshot.
    fn refresh_dirty(&mut self) {
        self.dirty = self.entries != self.original || self.comment != self.original_comment;
    }

    /// Reverts every entry and the archive comment to the open-time snapshot.
    pub(crate) fn unchange_all(&mut self) -> Result<(), ZipError> {
        self.writable()?;
        self.entries = self.original.clone();
        self.comment = self.original_comment.clone();
        self.refresh_dirty();
        self.success();
        Ok(())
    }

    /// Reverts one entry to its open-time snapshot.
    pub(crate) fn unchange_index(&mut self, index: usize) -> Result<(), ZipError> {
        self.writable()?;
        let Some(original) = self.original.get(index).cloned() else { return Err(ZipError::NoEnt) };
        if let Some(entry) = self.entries.get_mut(index) {
            *entry = original;
        }
        self.refresh_dirty();
        self.success();
        Ok(())
    }

    /// Reverts the archive comment and archive flags to the open-time state.
    pub(crate) fn unchange_archive(&mut self) -> Result<(), ZipError> {
        self.writable()?;
        self.comment = self.original_comment.clone();
        self.archive_flags &= !AFL_WANT_TORRENTZIP;
        self.refresh_dirty();
        self.success();
        Ok(())
    }
}

/// Returns whether an I/O failure is zip-rs's declared-uncompressed-size guard.
fn is_size_limit_error(error: &std::io::Error) -> bool {
    error.to_string().contains("larger than its declared uncompressed size")
}

/// Builds one model entry from a parsed central-directory record.
fn entry_from_raw(raw: &RawEntry, index: usize) -> Entry {
    let decoded = metadata::decode_name(&raw.name_raw, raw.flags);
    let method = raw.aes_method.map(i64::from).unwrap_or(i64::from(raw.method));
    Entry {
        name: decoded,
        name_raw: raw.name_raw.clone(),
        flags: raw.flags,
        method,
        method_explicit: true,
        size: raw.size,
        comp_size: raw.comp_size,
        crc: raw.crc,
        mtime: raw.mtime,
        comment: raw.comment.clone(),
        external_attrs: raw.external_attrs,
        system: i64::from(raw.system),
        encryption: raw.encryption,
        password: None,
        source: Source::Original(index),
        data: None,
        source_path: None,
        deleted: false,
        dirty: false,
        renamed: false,
    }
}

/// Decodes a user-supplied entry name according to the `FL_ENC_*` flags.
fn input_name(name: &[u8], flags: i64) -> Vec<u8> {
    if flags & FL_ENC_CP437 != 0 {
        crate::metadata::decode_cp437(name)
    } else {
        name.to_vec()
    }
}

/// Returns the final path segment of a stored name.
fn basename(name: &[u8]) -> &[u8] {
    name.rsplit(|&byte| byte == b'/').next().unwrap_or(name)
}
