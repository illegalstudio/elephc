//! Purpose:
//! Owns the mutable in-memory ZIP archive model behind `ZipArchive`.
//!
//! Called from:
//! - `crate::` FFI entry points, which expose it to the injected PHP prelude.
//!
//! Key details:
//! - Entries keep their original compressed payload so an unrelated mutation
//!   does not recompress (or corrupt) unchanged/encrypted entries.
//! - New and modified payloads are deflated with `flate2`; ZIP64 writing is
//!   rejected instead of emitting a truncated 32-bit archive.

use elephc_phar::{
    dos_to_unix, extract_zip_entry, open_zip_data, unix_to_dos, zip_entry_compressed_payload,
};

/// Success result for mutating bridge calls.
pub const ER_OK: i64 = 0;
/// Libzip `ZIP_ER_COMPNOTSUPP`.
pub const ER_COMPNOTSUPP: i64 = 16;
/// Libzip `ZIP_ER_INVAL`.
pub const ER_INVAL: i64 = 18;
/// Libzip `ZIP_ER_NOZIP`.
pub const ER_NOZIP: i64 = 19;
/// Libzip `ZIP_ER_RDONLY`.
pub const ER_RDONLY: i64 = 25;
/// Libzip `ZIP_ER_OPNOTSUPP`.
pub const ER_OPNOTSUPP: i64 = 28;
/// Libzip `ZIP_ER_NOENT`.
pub const ER_NOENT: i64 = 9;
/// Libzip `ZIP_ER_EXISTS`.
pub const ER_EXISTS: i64 = 10;

/// ZIP compression method: stored.
pub const METHOD_STORE: u16 = 0;
/// ZIP compression method: deflated.
pub const METHOD_DEFLATE: u16 = 8;
/// `ZipArchive::FL_OVERWRITE`, the default for add operations.
pub const FL_OVERWRITE: i64 = 8192;
/// `ZipArchive::FL_NOCASE`.
pub const FL_NOCASE: i64 = 1;
/// `ZipArchive::FL_NODIR`.
pub const FL_NODIR: i64 = 2;

/// One in-memory archive entry.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Raw entry name bytes.
    pub name: Vec<u8>,
    /// Uncompressed payload, or `None` when the entry is encrypted/unsupported.
    pub data: Option<Vec<u8>>,
    /// Original compressed payload; reused verbatim while the entry is unchanged.
    pub compressed: Option<Vec<u8>>,
    /// Compression method (0 = store, 8 = deflate).
    pub method: u16,
    /// Original general-purpose flags (encryption/UTF-8 bits are preserved).
    pub flags: u16,
    /// MS-DOS modification time.
    pub dos_time: u16,
    /// MS-DOS modification date.
    pub dos_date: u16,
    /// CRC-32 of the uncompressed payload.
    pub crc32: u32,
    /// Uncompressed size from the central directory.
    pub uncompressed_size: usize,
    /// Entry comment bytes.
    pub comment: Vec<u8>,
    /// External file attributes.
    pub external_attrs: u32,
}

/// A mutable in-memory ZIP archive.
#[derive(Clone, Debug)]
pub struct Archive {
    /// Original archive bytes, returned unchanged when nothing was modified.
    original: Vec<u8>,
    /// Entries in archive order.
    pub entries: Vec<Entry>,
    /// Archive comment bytes.
    pub comment: Vec<u8>,
    /// Whether a mutation requires reserialization.
    pub dirty: bool,
}

impl Archive {
    /// Opens a raw ZIP archive, eagerly decoding every supported entry.
    pub fn open(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return Some(Self::empty());
        }
        let parsed = open_zip_data(bytes)?;
        let entries = parsed
            .entries
            .iter()
            .enumerate()
            .map(|(index, info)| Entry {
                name: info.name.clone(),
                data: extract_zip_entry(&parsed, index),
                compressed: zip_entry_compressed_payload(&parsed, index),
                method: info.method,
                flags: info.flags,
                dos_time: info.dos_time,
                dos_date: info.dos_date,
                crc32: info.crc32,
                uncompressed_size: info.uncompressed_size,
                comment: info.comment.clone(),
                external_attrs: info.external_attrs,
            })
            .collect();
        Some(Self {
            original: bytes.to_vec(),
            entries,
            comment: parsed.comment,
            dirty: false,
        })
    }

    /// Creates a new empty archive.
    pub fn empty() -> Self {
        Self {
            original: Vec::new(),
            entries: Vec::new(),
            comment: Vec::new(),
            dirty: true,
        }
    }

    /// Returns the entry count.
    pub fn num_files(&self) -> usize {
        self.entries.len()
    }

    /// Finds an entry by name, honoring `FL_NOCASE` and `FL_NODIR`.
    pub fn locate(&self, name: &[u8], flags: i64) -> Option<usize> {
        let nocase = flags & FL_NOCASE != 0;
        let nodir = flags & FL_NODIR != 0;
        self.entries.iter().position(|entry| {
            let candidate = if nodir {
                base_name(&entry.name)
            } else {
                &entry.name
            };
            if nocase {
                candidate.eq_ignore_ascii_case(name)
            } else {
                candidate == name
            }
        })
    }

    /// Returns one entry's uncompressed bytes.
    pub fn payload(&self, index: usize) -> Option<&[u8]> {
        self.entries.get(index)?.data.as_deref()
    }

    /// Returns one entry's comment bytes.
    pub fn entry_comment(&self, index: usize) -> Option<&[u8]> {
        self.entries.get(index).map(|entry| entry.comment.as_slice())
    }

    /// Returns one entry's Unix modification timestamp.
    pub fn entry_mtime(&self, index: usize) -> Option<i64> {
        let entry = self.entries.get(index)?;
        Some(dos_to_unix(entry.dos_time, entry.dos_date))
    }

    /// Adds or overwrites a file entry.
    pub fn add_file(
        &mut self,
        name: Vec<u8>,
        data: Vec<u8>,
        flags: i64,
        mtime: i64,
    ) -> Result<usize, i64> {
        if name.is_empty() {
            return Err(ER_INVAL);
        }
        let (dos_time, dos_date) = unix_to_dos(mtime);
        if let Some(index) = self.locate(&name, 0) {
            if flags & FL_OVERWRITE == 0 {
                return Err(ER_EXISTS);
            }
            self.entries[index] = new_entry(name, data, dos_time, dos_date, false);
            self.dirty = true;
            return Ok(index);
        }
        self.entries
            .push(new_entry(name, data, dos_time, dos_date, false));
        self.dirty = true;
        Ok(self.entries.len() - 1)
    }

    /// Adds a directory entry, forcing a trailing slash.
    pub fn add_dir(&mut self, mut name: Vec<u8>, flags: i64, mtime: i64) -> Result<usize, i64> {
        if name.is_empty() {
            return Err(ER_INVAL);
        }
        if name.last() != Some(&b'/') {
            name.push(b'/');
        }
        let (dos_time, dos_date) = unix_to_dos(mtime);
        if let Some(index) = self.locate(&name, 0) {
            if flags & FL_OVERWRITE == 0 {
                return Err(ER_EXISTS);
            }
            self.entries[index] = new_entry(name, Vec::new(), dos_time, dos_date, true);
            self.dirty = true;
            return Ok(index);
        }
        self.entries
            .push(new_entry(name, Vec::new(), dos_time, dos_date, true));
        self.dirty = true;
        Ok(self.entries.len() - 1)
    }

    /// Deletes an entry by index.
    pub fn delete_index(&mut self, index: usize) -> i64 {
        if index >= self.entries.len() {
            return ER_INVAL;
        }
        self.entries.remove(index);
        self.dirty = true;
        ER_OK
    }

    /// Renames an entry by index.
    pub fn rename_index(&mut self, index: usize, name: Vec<u8>) -> i64 {
        if index >= self.entries.len() || name.is_empty() {
            return ER_INVAL;
        }
        if let Some(existing) = self.locate(&name, 0) {
            if existing != index {
                return ER_EXISTS;
            }
        }
        self.entries[index].name = name;
        self.dirty = true;
        ER_OK
    }

    /// Sets an entry's comment by index.
    pub fn set_entry_comment(&mut self, index: usize, comment: Vec<u8>) -> i64 {
        let Some(entry) = self.entries.get_mut(index) else {
            return ER_INVAL;
        };
        entry.comment = comment;
        self.dirty = true;
        ER_OK
    }

    /// Sets an entry's compression method by index.
    pub fn set_compression(&mut self, index: usize, method: u16) -> i64 {
        if method != METHOD_STORE && method != METHOD_DEFLATE {
            return ER_COMPNOTSUPP;
        }
        let Some(entry) = self.entries.get_mut(index) else {
            return ER_INVAL;
        };
        if let Some(data) = entry.data.as_deref() {
            let (compressed, crc32) = compress(data, method);
            entry.compressed = Some(compressed);
            entry.crc32 = crc32;
        }
        entry.method = method;
        self.dirty = true;
        ER_OK
    }

    /// Sets an entry's modification time by index.
    pub fn set_mtime(&mut self, index: usize, timestamp: i64) -> i64 {
        let Some(entry) = self.entries.get_mut(index) else {
            return ER_INVAL;
        };
        let (dos_time, dos_date) = unix_to_dos(timestamp);
        entry.dos_time = dos_time;
        entry.dos_date = dos_date;
        self.dirty = true;
        ER_OK
    }

    /// Serializes the archive for `closeString`.
    pub fn close_string(&self) -> Option<Vec<u8>> {
        if !self.dirty {
            return Some(self.original.clone());
        }
        crate::write::serialize(self)
    }
}

/// Builds a fresh file or directory entry and compresses its payload.
fn new_entry(
    name: Vec<u8>,
    data: Vec<u8>,
    dos_time: u16,
    dos_date: u16,
    is_dir: bool,
) -> Entry {
    let method = if is_dir || data.is_empty() {
        METHOD_STORE
    } else {
        METHOD_DEFLATE
    };
    let (compressed, crc32) = compress(&data, method);
    Entry {
        name,
        uncompressed_size: data.len(),
        data: Some(data),
        compressed: Some(compressed),
        method,
        flags: if is_dir { 0 } else { 0x0800 },
        dos_time,
        dos_date,
        crc32,
        comment: Vec::new(),
        external_attrs: if is_dir {
            0x10 | (0o40755 << 16)
        } else {
            0o100644 << 16
        },
    }
}

/// Compresses a payload with the requested method and returns `(bytes, crc32)`.
pub fn compress(data: &[u8], method: u16) -> (Vec<u8>, u32) {
    let crc32 = crc32(data);
    if method == METHOD_DEFLATE && !data.is_empty() {
        use std::io::Write;
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        if encoder.write_all(data).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                return (compressed, crc32);
            }
        }
    }
    (data.to_vec(), crc32)
}

/// Returns the CRC-32 (IEEE) of `data`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Returns the final path component of a raw ZIP name.
fn base_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&byte| byte == b'/') {
        Some(index) => &name[index + 1..],
        None => name,
    }
}
