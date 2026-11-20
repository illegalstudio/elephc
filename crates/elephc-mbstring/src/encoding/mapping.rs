//! Purpose:
//! Reads compact generated codec mapping records without alignment assumptions.
//!
//! Called from:
//! - The legacy multibyte and mobile UTF-8 codec implementations.
//!
//! Key details:
//! - Raw index records are sorted little-endian key/offset pairs, read by [`lookup`].
//! - Large codec indexes are embedded through [`index_table!`] in the blocked delta layout
//!   written by the crate's `build.rs`, read by [`Index`]. Both return the same value for
//!   every key; the tests below compare each compacted table with its canonical file.
//! - Large dense word arrays are embedded through [`dense_table!`] as page directories into
//!   one page pool shared by all of them, read by [`Dense`].

/// Embeds the compacted form of one sorted index file from `src/encoding/data`.
///
/// `build.rs` finds these literals in the generated catalog and writes their compacted
/// copies to `OUT_DIR`, so only the compact bytes reach the binary.
macro_rules! index_table {
    ($file:literal) => {
        $crate::encoding::mapping::Index::new(include_bytes!(concat!(env!("OUT_DIR"), "/mbstring-index/", $file)))
    };
}
pub(super) use index_table;

/// Embeds the page directory of one dense word array from `src/encoding/data`.
macro_rules! dense_table {
    ($file:literal) => {
        $crate::encoding::mapping::Dense::new(include_bytes!(concat!(env!("OUT_DIR"), "/mbstring-dense/", $file)))
    };
}
pub(super) use dense_table;

/// Distinct 64-word pages shared by every dense table, written by `build.rs`.
static DENSE_POOL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mbstring-dense/pool.bin"));
/// Words in one shared dense page, matching `build.rs`.
const PAGE_WORDS: usize = 64;

/// A dense little-endian word array stored as pages of the shared pool.
#[derive(Clone, Copy, Debug)]
pub(super) struct Dense(&'static [u8]);

impl Dense {
    /// Wraps a page directory produced by `build.rs`.
    pub(crate) const fn new(directory: &'static [u8]) -> Self {
        Self(directory)
    }

    /// Returns word `index`, as [`word`] does at byte offset `index * 4` of the canonical file.
    ///
    /// Panics outside the table, like a slice read past the canonical file.
    pub(super) fn word(self, index: usize) -> u32 {
        let len = word(self.0, 0) as usize;
        assert!(index < len, "dense codec table read {index} past its {len} words");
        let entry = 4 + (index / PAGE_WORDS) * 2;
        let page = usize::from(u16::from_le_bytes([self.0[entry], self.0[entry + 1]]));
        word(DENSE_POOL, (page * PAGE_WORDS + index % PAGE_WORDS) * 4)
    }
}

/// Records per directory block, matching `build.rs`.
const BLOCK: usize = 16;
/// Bytes in one directory entry: first key, first value, stream offset.
const ENTRY: usize = 12;
/// Bytes before the directory: record count and block count.
const HEADER: usize = 8;

/// A sorted codepoint/byte-sequence index in the compact blocked delta layout.
#[derive(Clone, Copy, Debug)]
pub(super) struct Index(&'static [u8]);

impl Index {
    /// Wraps compacted bytes produced by `build.rs`.
    pub(crate) const fn new(bytes: &'static [u8]) -> Self {
        Self(bytes)
    }

    /// Reports whether the index has no records.
    pub(super) fn is_empty(self) -> bool {
        self.count() == 0
    }

    /// Returns the value stored for `key`, as [`lookup`] does on the canonical records.
    pub(super) fn get(self, key: u32) -> Option<u32> {
        let count = self.count();
        if count == 0 {
            return None;
        }
        let blocks = word(self.0, 4) as usize;
        let (mut low, mut high) = (0, blocks);
        while low < high {
            let mid = low + (high - low) / 2;
            if word(self.0, HEADER + mid * ENTRY) <= key { low = mid + 1; } else { high = mid; }
        }
        let block = low.checked_sub(1)?;
        let entry = HEADER + block * ENTRY;
        let (mut current, mut value) = (u64::from(word(self.0, entry)), i64::from(word(self.0, entry + 4)));
        let mut cursor = HEADER + blocks * ENTRY + word(self.0, entry + 8) as usize;
        for _ in 1..(count - block * BLOCK).min(BLOCK) {
            if current == u64::from(key) {
                break;
            }
            let gap = varint(self.0, &mut cursor);
            let delta = varint(self.0, &mut cursor);
            current += gap + 1;
            value += ((delta >> 1) as i64) ^ -((delta & 1) as i64);
            if current > u64::from(key) {
                return None;
            }
        }
        (current == u64::from(key)).then_some(value as u32)
    }

    /// Returns the number of records.
    fn count(self) -> usize {
        word(self.0, 0) as usize
    }
}

/// Decodes one unsigned LEB128 integer and advances the cursor past it.
fn varint(bytes: &[u8], cursor: &mut usize) -> u64 {
    let (mut value, mut shift) = (0u64, 0);
    loop {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}

/// Finds a codepoint's payload offset in a sorted array of key/offset records.
pub(super) fn lookup(index: &[u8], key: u32) -> Option<u32> {
    let (mut low, mut high) = (0, index.len() / 8);
    while low < high {
        let mid = low + (high - low) / 2;
        match word(index, mid * 8).cmp(&key) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Some(word(index, mid * 8 + 4)),
        }
    }
    None
}

/// Reads one generated little-endian word without alignment assumptions.
pub(super) fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("generated codec word"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every compacted index with the canonical generated file it was built from.
    const TABLES: &[(&str, &[u8], &[u8])] = include!(concat!(env!("OUT_DIR"), "/mbstring_index_tables.rs"));

    /// Answers every stored key, both neighbours of each key, and the whole 16-bit key space
    /// exactly as the canonical sorted records do.
    #[test]
    fn compact_indexes_match_their_canonical_records() {
        assert!(TABLES.len() > 40, "catalog lost its index_table! entries: {}", TABLES.len());
        let compact_bytes: usize = TABLES.iter().map(|(_, compact, _)| compact.len()).sum();
        let raw_bytes: usize = TABLES.iter().map(|(_, _, raw)| raw.len()).sum();
        assert!(compact_bytes * 2 < raw_bytes, "compaction no longer halves the indexes: {compact_bytes}/{raw_bytes}");
        for &(name, compact, raw) in TABLES {
            let index = Index::new(compact);
            assert_eq!(index.is_empty(), raw.is_empty(), "{name}");
            let mut probes: Vec<u32> = (0..=0xffff).chain([u32::MAX, u32::MAX - 1]).collect();
            for record in raw.chunks_exact(8) {
                let key = word(record, 0);
                assert_eq!(index.get(key), Some(word(record, 4)), "{name}: key {key:#x}");
                probes.extend([key.wrapping_sub(1), key.wrapping_add(1)]);
            }
            for key in probes {
                assert_eq!(index.get(key), lookup(raw, key), "{name}: probe {key:#x}");
            }
        }
    }

    /// Every pooled dense table with the canonical generated file it was built from.
    const DENSE_TABLES: &[(&str, &[u8], &[u8])] = include!(concat!(env!("OUT_DIR"), "/mbstring_dense_tables.rs"));

    /// Reads every word of every dense table exactly as the canonical file holds it.
    #[test]
    fn pooled_dense_tables_match_their_canonical_words() {
        assert!(DENSE_TABLES.len() >= 10, "catalog lost its dense_table! entries: {}", DENSE_TABLES.len());
        let raw_bytes: usize = DENSE_TABLES.iter().map(|(_, _, raw)| raw.len()).sum();
        let pooled_bytes = DENSE_POOL.len() + DENSE_TABLES.iter().map(|(_, table, _)| table.len()).sum::<usize>();
        assert!(pooled_bytes < raw_bytes * 3 / 4, "page pooling no longer pays: {pooled_bytes}/{raw_bytes}");
        for &(name, directory, raw) in DENSE_TABLES {
            let table = Dense::new(directory);
            assert_eq!(word(directory, 0) as usize, raw.len() / 4, "{name}");
            for index in 0..raw.len() / 4 {
                assert_eq!(table.word(index), word(raw, index * 4), "{name}: word {index}");
            }
        }
    }

    /// Rejects reads past a dense table the way the canonical slice read did.
    #[test]
    #[should_panic(expected = "past its")]
    fn pooled_dense_table_rejects_reads_past_its_end() {
        let (_, directory, raw) = DENSE_TABLES[0];
        Dense::new(directory).word(raw.len() / 4);
    }

    /// Keeps an empty generated index empty, as the triple decoder's guard requires.
    #[test]
    fn empty_index_answers_nothing() {
        let empty = Index::new(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);
        assert_eq!(empty.get(u32::MAX), None);
    }
}
