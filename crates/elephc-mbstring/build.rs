//! Purpose:
//! Compacts the generated sorted codec mapping indexes before they are embedded in the crate.
//!
//! Called from:
//! - Cargo before building the shared mbstring engine.
//!
//! Key details:
//! - The canonical `src/encoding/data/*.bin` files stay the generator's output: sorted
//!   little-endian `(key, value)` u32 records. Only files named by an `index_table!` literal in
//!   the generated catalog are compacted; payload and dense tables are embedded unchanged.
//! - Compact index layout, all little-endian: `count: u32`, `blocks: u32`, then one 16-byte
//!   directory entry per block of up to 32 records (`first_key: u32`, `value_base: u32`,
//!   `data_offset: u32`, `key_width: u8`, `value_width: u8`, `records: u8`, padding), then the
//!   block data: every key as `key - first_key` in `key_width` bytes, followed by every value
//!   as `value - value_base` in `value_width` bytes. Fixed widths keep each block searchable
//!   by bisection, so a lookup never decodes records sequentially.
//! - Files named by a `dense_table!` literal are little-endian u32 arrays indexed directly.
//!   They are cut into 64-word pages and every distinct page is stored once in a pool shared
//!   by all dense tables (`mbstring-dense/pool.bin`). Each table keeps `len_words: u32`
//!   followed by one little-endian u16 pool page id per page; a short final page is
//!   zero-padded in the pool and bounded by `len_words`.
//! - `crate::encoding::mapping::{Index, Dense}` read these layouts; their tests compare every
//!   compacted table with the canonical file it came from.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Records per directory block, searched by bisection after the directory.
const BLOCK: usize = 32;
/// Macro whose string literal names a sorted index file in the generated catalog.
const INDEX_MARKER: &str = "index_table!(\"";
/// Macro whose string literal names a dense word array in the generated catalog.
const DENSE_MARKER: &str = "dense_table!(\"";
/// Words in one shared dense page.
const PAGE_WORDS: usize = 64;

/// Compacts every catalog index and dense table and lists them for the equivalence tests.
fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let catalog = manifest.join("src/encoding/catalog_data.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", catalog.display());
    let source = std::fs::read_to_string(&catalog).expect("generated mbstring catalog is readable");
    let directory = out.join("mbstring-index");
    std::fs::create_dir_all(&directory).expect("index output directory can be created");
    let mut listing = String::from("&[\n");
    for name in table_names(&source, INDEX_MARKER) {
        let raw_path = manifest.join("src/encoding/data").join(&name);
        println!("cargo:rerun-if-changed={}", raw_path.display());
        let raw = std::fs::read(&raw_path).unwrap_or_else(|error| panic!("{}: {error}", raw_path.display()));
        let compact_path = directory.join(&name);
        write_if_changed(&compact_path, &compact(&raw, &name));
        writeln!(listing, "    ({name:?}, include_bytes!({:?}), include_bytes!({:?})),",
            compact_path.display().to_string(), raw_path.display().to_string()).expect("string write");
    }
    listing.push_str("]\n");
    write_if_changed(&out.join("mbstring_index_tables.rs"), listing.as_bytes());
    write_dense_tables(&manifest, &out, &source);
}

/// Pools the pages of every dense table and lists the tables for the equivalence tests.
fn write_dense_tables(manifest: &Path, out: &Path, source: &str) {
    let directory = out.join("mbstring-dense");
    std::fs::create_dir_all(&directory).expect("dense output directory can be created");
    let mut pool: Vec<u8> = Vec::new();
    let mut pages: std::collections::HashMap<Vec<u8>, u16> = std::collections::HashMap::new();
    let mut listing = String::from("&[\n");
    for name in table_names(source, DENSE_MARKER) {
        let raw_path = manifest.join("src/encoding/data").join(&name);
        println!("cargo:rerun-if-changed={}", raw_path.display());
        let raw = std::fs::read(&raw_path).unwrap_or_else(|error| panic!("{}: {error}", raw_path.display()));
        assert_eq!(raw.len() % 4, 0, "{name}: dense tables hold whole words");
        let mut table = u32::try_from(raw.len() / 4).expect("dense length fits u32").to_le_bytes().to_vec();
        for chunk in raw.chunks(PAGE_WORDS * 4) {
            let mut page = chunk.to_vec();
            page.resize(PAGE_WORDS * 4, 0);
            let next = u16::try_from(pages.len()).expect("dense page pool fits u16 ids");
            let id = *pages.entry(page.clone()).or_insert_with(|| {
                pool.extend_from_slice(&page);
                next
            });
            table.extend_from_slice(&id.to_le_bytes());
        }
        let compact_path = directory.join(&name);
        write_if_changed(&compact_path, &table);
        writeln!(listing, "    ({name:?}, include_bytes!({:?}), include_bytes!({:?})),",
            compact_path.display().to_string(), raw_path.display().to_string()).expect("string write");
    }
    listing.push_str("]\n");
    write_if_changed(&directory.join("pool.bin"), &pool);
    write_if_changed(&out.join("mbstring_dense_tables.rs"), listing.as_bytes());
}

/// Returns each distinct data file named by one table macro in the catalog, in sorted order.
fn table_names(source: &str, marker: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(marker) {
        rest = &rest[start + marker.len()..];
        let end = rest.find('"').expect("table macro literal is terminated");
        let name = &rest[..end];
        assert!(!name.contains(['/', '\\']), "table macros name a file inside src/encoding/data: {name}");
        names.push(name.to_string());
        rest = &rest[end..];
    }
    names.sort();
    names.dedup();
    names
}

/// Encodes sorted `(key, value)` records into fixed-width offset blocks.
fn compact(raw: &[u8], name: &str) -> Vec<u8> {
    assert_eq!(raw.len() % 8, 0, "{name}: index records are eight bytes");
    let records: Vec<(u32, u32)> = raw.chunks_exact(8).map(|record| (word(record, 0), word(record, 4))).collect();
    assert!(records.windows(2).all(|pair| pair[0].0 < pair[1].0), "{name}: index keys must be strictly ascending");
    let blocks = records.len().div_ceil(BLOCK);
    let mut directory = Vec::with_capacity(blocks * 16);
    let mut data = Vec::new();
    for block in records.chunks(BLOCK) {
        let first_key = block[0].0;
        let value_base = block.iter().map(|record| record.1).min().expect("blocks are non-empty");
        let key_width = byte_width(block[block.len() - 1].0 - first_key);
        let value_width = byte_width(block.iter().map(|record| record.1 - value_base).max().expect("blocks are non-empty"));
        directory.extend_from_slice(&first_key.to_le_bytes());
        directory.extend_from_slice(&value_base.to_le_bytes());
        directory.extend_from_slice(&u32::try_from(data.len()).expect("index data fits u32").to_le_bytes());
        directory.extend_from_slice(&[key_width as u8, value_width as u8, block.len() as u8, 0]);
        for &(key, _) in block {
            data.extend_from_slice(&(key - first_key).to_le_bytes()[..key_width]);
        }
        for &(_, value) in block {
            data.extend_from_slice(&(value - value_base).to_le_bytes()[..value_width]);
        }
    }
    let mut output = Vec::with_capacity(8 + directory.len() + data.len());
    output.extend_from_slice(&u32::try_from(records.len()).expect("record count fits u32").to_le_bytes());
    output.extend_from_slice(&u32::try_from(blocks).expect("block count fits u32").to_le_bytes());
    output.extend_from_slice(&directory);
    output.extend_from_slice(&data);
    output
}

/// Returns the fewest little-endian bytes, one to four, that hold `value`.
fn byte_width(value: u32) -> usize {
    match value {
        0..=0xff => 1,
        0x100..=0xffff => 2,
        0x1_0000..=0xff_ffff => 3,
        _ => 4,
    }
}

/// Reads one little-endian word from a generated record.
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("generated codec word"))
}

/// Rewrites an output only when its bytes change, keeping dependent rebuilds incremental.
fn write_if_changed(path: &Path, bytes: &[u8]) {
    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        return;
    }
    std::fs::write(path, bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
}
