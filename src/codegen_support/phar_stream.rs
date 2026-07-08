//! Purpose:
//! Parses `phar://` URLs and PHAR archive metadata for EIR I/O lowering.
//! Provides compile-time entry extraction and write-template construction.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io` for literal PHAR read/write paths.
//!
//! Key details:
//! - The URL must be a string literal. The archive file is read and parsed at
//!   compile time (relative paths resolve against the compiler's working
//!   directory), the requested entry's uncompressed bytes are embedded in the
//!   binary's data section, and reads come from that embedded copy — mirroring
//!   how `data://` lowers a literal payload. Read-only entries from native PHAR,
//!   tar-based PHAR, and zip-based PHAR containers are supported; native gzip
//!   (raw-DEFLATE), native bzip2, and zip deflate entries are decompressed at
//!   compile time.
//! - A missing archive or a missing entry lowers to PHP `false`, matching a
//!   failed `fopen()`.
//! - Write-mode literal URLs seed the shared PHAR write runtime. The splitter
//!   recognizes `.phar/`, `.tar/`, and `.zip/` archive boundaries so the
//!   runtime bridge can preserve the requested archive family.
//! - PHAR binary layout parsed here (all integers little-endian): a PHP stub
//!   ending in `__HALT_COMPILER();`, then the manifest
//!   (`manifest_len`, `num_files`, 2-byte api version, 4-byte global flags,
//!   `alias_len`+alias, `meta_len`+metadata, then per file:
//!   `name_len`+name, `uncompressed_size`, timestamp, `compressed_size`, crc32,
//!   `flags`, `meta_len`+metadata), then the file-data section beginning at
//!   `manifest_start + 4 + manifest_len`, holding each entry's bytes
//!   consecutively in manifest order.

/// PHAR per-entry flag bit: the entry's data is stored as raw DEFLATE (what PHP
/// writes for gzip-compressed entries — no zlib or gzip header).
const PHAR_FLAG_GZIP: u32 = 0x0000_1000;
/// PHAR per-entry flag bit: the entry's data is bzip2 compressed.
const PHAR_FLAG_BZIP2: u32 = 0x0000_2000;

/// Splits a `phar://<archive>/<entry>` write URL into `(archive_path, entry)`.
/// Unlike the read path the archive need not exist yet, so the split happens at
/// the first `.phar/`, `.tar/`, or `.zip/` boundary; if none is present it
/// falls back to the longest existing-file prefix. Returns `None` when neither
/// rule yields an entry.
pub(crate) fn resolve_write_target(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("phar://")?;
    for suffix in [".phar/", ".tar/", ".zip/"] {
        if let Some(idx) = rest.find(suffix) {
            let archive_end = idx + suffix.len() - 1;
            let archive = &rest[..archive_end];
            let entry = &rest[archive_end + 1..];
            if !entry.is_empty() {
                return Some((archive.to_string(), entry.to_string()));
            }
        }
    }
    let (archive, entry) = split_archive_entry(rest)?;
    let entry = entry.strip_prefix('/').unwrap_or(entry);
    Some((archive.to_string(), entry.to_string()))
}

/// Extracts the bytes for a literal `phar://` URL from a native, tar-based, or
/// zip-based PHAR archive.
pub(crate) fn extract_phar_entry(url: &str) -> Option<Vec<u8>> {
    if let Some(bytes) = elephc_phar::extract_url_bytes(url.as_bytes()) {
        return Some(bytes);
    }
    let rest = url.strip_prefix("phar://")?;
    let (archive, entry) = split_archive_entry(rest)?;
    let archive_bytes = std::fs::read(archive).ok()?;
    let entry = entry.strip_prefix('/').unwrap_or(entry);
    parse_phar_entry(&archive_bytes, entry)
}

/// Splits a `phar://` body into `(archive_path, inner_entry)` by taking the
/// shortest `/`-delimited prefix that names an existing file as the archive —
/// the same disambiguation PHP uses to find where the archive ends and the
/// entry begins. Returns `None` if no prefix is an existing file.
fn split_archive_entry(rest: &str) -> Option<(&str, &str)> {
    for (i, &c) in rest.as_bytes().iter().enumerate() {
        if c == b'/' {
            let candidate = &rest[..i];
            if std::path::Path::new(candidate).is_file() {
                return Some((candidate, &rest[i + 1..]));
            }
        }
    }
    None
}

/// Parses the native PHAR manifest in `data` and returns the uncompressed bytes
/// of `entry`, or `None` if the archive is malformed or the entry is absent.
fn parse_phar_entry(data: &[u8], entry: &str) -> Option<Vec<u8>> {
    let halt = b"__HALT_COMPILER();";
    let halt_idx = find_subslice(data, halt)?;
    let mut p = halt_idx + halt.len();
    // PHP writes `__HALT_COMPILER(); ?>\r\n`; skip each of those bytes in order
    // when present, leaving `p` at the first manifest byte.
    for &ch in &[b' ', b'?', b'>', b'\r', b'\n'] {
        if data.get(p) == Some(&ch) {
            p += 1;
        }
    }

    let manifest_start = p;
    let manifest_len = le32(data, manifest_start)? as usize;
    let data_section = manifest_start.checked_add(4)?.checked_add(manifest_len)?;
    let num_files = le32(data, manifest_start + 4)?;

    // Skip the rest of the manifest header: api version (2) + global flags (4) +
    // alias (len-prefixed) + manifest metadata (len-prefixed).
    let mut q = manifest_start + 8 + 2 + 4;
    let alias_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(alias_len)?;
    let meta_len = le32(data, q)? as usize;
    q = q.checked_add(4)?.checked_add(meta_len)?;

    // Walk each entry, accumulating the running data-section offset so a matched
    // entry's bytes can be sliced even when earlier entries precede it.
    let mut data_offset = 0usize;
    for _ in 0..num_files {
        let name_len = le32(data, q)? as usize;
        q += 4;
        let name = data.get(q..q.checked_add(name_len)?)?;
        q += name_len;
        let uncompressed = le32(data, q)? as usize;
        q += 4; // uncompressed size
        q += 4; // timestamp
        let compressed = le32(data, q)? as usize;
        q += 4; // compressed size
        q += 4; // crc32
        let flags = le32(data, q)?;
        q += 4;
        let entry_meta_len = le32(data, q)? as usize;
        q = q.checked_add(4)?.checked_add(entry_meta_len)?;

        if name == entry.as_bytes() {
            let start = data_section.checked_add(data_offset)?;
            let stored = data.get(start..start.checked_add(compressed)?)?;
            return decode_entry(stored, flags, uncompressed);
        }
        data_offset = data_offset.checked_add(compressed)?;
    }
    None
}

/// Decodes a stored PHAR entry payload into its uncompressed bytes according to
/// the entry `flags`: raw-DEFLATE for gzip entries and bzip2 for bzip2 entries
/// (each verified against the entry's recorded `uncompressed` size), passthrough
/// for uncompressed entries, and `None` on a malformed compressed stream.
fn decode_entry(stored: &[u8], flags: u32, uncompressed: usize) -> Option<Vec<u8>> {
    if flags & PHAR_FLAG_GZIP != 0 {
        let mut out = Vec::with_capacity(uncompressed);
        let mut decoder = flate2::read::DeflateDecoder::new(stored);
        std::io::Read::read_to_end(&mut decoder, &mut out).ok()?;
        if out.len() != uncompressed {
            return None; // recorded size disagrees with the inflated length
        }
        Some(out)
    } else if flags & PHAR_FLAG_BZIP2 != 0 {
        let mut out = Vec::with_capacity(uncompressed);
        let mut decoder = bzip2_rs::DecoderReader::new(stored);
        std::io::Read::read_to_end(&mut decoder, &mut out).ok()?;
        if out.len() != uncompressed {
            return None; // recorded size disagrees with the decompressed length
        }
        Some(out)
    } else {
        Some(stored.to_vec())
    }
}

/// Reads a little-endian `u32` at `off`, or `None` if fewer than 4 bytes remain.
fn le32(data: &[u8], off: usize) -> Option<u32> {
    let b = data.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Returns the index of the first occurrence of `needle` in `hay`, or `None`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Builds the native-PHAR prefix for a single uncompressed `entry`, stored at
/// `0644`. The global manifest flags set `PHAR_HDR_SIGNATURE` (0x10000), so the
/// archive declares an appended signature; `__rt_phar_write_finalize` computes a
/// SHA1 signature over the assembled bytes and appends the
/// `raw-sha1 ++ LE32(0x0002) ++ "GBMB"` trailer, making the archive readable by
/// real PHP (which requires a hash by default), not just elephc. The returned
/// bytes are everything up to the data section; the runtime appends the entry
/// content and patches the size/CRC fields, which sit at fixed negative offsets
/// from the end of the template (uncompressed at -24, compressed at -16, crc at
/// -12) — so `__rt_phar_write_finalize` derives them from the template length.
pub(crate) fn build_phar_write_template(entry: &str) -> Vec<u8> {
    let name = entry.as_bytes();
    let mut out = Vec::new();
    out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
    // manifest length = every byte after this LE32 up to the data section:
    // num_files(4)+api(2)+flags(4)+alias_len(4)+meta_len(4) + the entry record
    // (name_len(4)+name + uncomp(4)+ts(4)+comp(4)+crc(4)+flags(4)+emeta(4)).
    let manifest_len = (18 + name.len() + 28) as u32;
    out.extend_from_slice(&manifest_len.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // num_files
    out.extend_from_slice(&[0x11, 0x00]); // api version (1.1.0)
    out.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // global flags: PHAR_HDR_SIGNATURE (signed; trailer appended by finalize)
    out.extend_from_slice(&0u32.to_le_bytes()); // alias length
    out.extend_from_slice(&0u32.to_le_bytes()); // manifest metadata length
    out.extend_from_slice(&(name.len() as u32).to_le_bytes()); // entry name length
    out.extend_from_slice(name); // entry name
    out.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size (runtime patch, -24)
    out.extend_from_slice(&0u32.to_le_bytes()); // timestamp
    out.extend_from_slice(&0u32.to_le_bytes()); // compressed size (runtime patch, -16)
    out.extend_from_slice(&0u32.to_le_bytes()); // crc32 (runtime patch, -12)
    out.extend_from_slice(&0x0000_01a4u32.to_le_bytes()); // flags: mode 0644, uncompressed
    out.extend_from_slice(&0u32.to_le_bytes()); // entry metadata length
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips the write-side template through the read-side parser: build a
    /// template, simulate the runtime finalize (patch sizes, append content),
    /// and confirm `parse_phar_entry` extracts the same bytes back. The reader
    /// ignores CRC, so the crc field is left zero here.
    #[test]
    fn write_template_round_trips_through_reader() {
        let content = b"hello from a written phar entry";
        let tpl = build_phar_write_template("dir/inner.txt");
        let tpl_len = tpl.len();
        let mut archive = tpl.clone();
        let len = (content.len() as u32).to_le_bytes();
        // finalize patches uncompressed at tpl_len-24 and compressed at tpl_len-16
        // (crc at tpl_len-12, left zero here because the reader ignores it).
        archive[tpl_len - 24..tpl_len - 20].copy_from_slice(&len);
        archive[tpl_len - 16..tpl_len - 12].copy_from_slice(&len);
        archive.extend_from_slice(content);

        assert_eq!(
            parse_phar_entry(&archive, "dir/inner.txt").as_deref(),
            Some(&content[..])
        );
        assert!(parse_phar_entry(&archive, "absent.txt").is_none());
    }
}
