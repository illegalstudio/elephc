//! Purpose:
//! IPTC IIM metadata side of the bridge: parse a raw IPTC block into datasets and
//! embed an IPTC block into a JPEG as a Photoshop APP13 marker. Backs PHP's
//! `iptcparse` and `iptcembed`. There is no mature pure-Rust IPTC crate, so the
//! IIM record format and the 8BIM/APP13 container are handled by hand here.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`.
//!   Both inputs (the IPTC block / the IPTC data) arrive through the shared input
//!   staging buffer (`crate::xfer`); `iptcparse` results come back through the
//!   shared key/value list and the embedded JPEG through the shared output buffer.
//!
//! Key details:
//! - An IIM dataset is `0x1C, record, dataset, length(2 BE)`, then `length` bytes.
//!   A length with the high bit set is the extended form: the low 15 bits give the
//!   byte count of the real length that follows. Keys are PHP's `record#dataset`
//!   with a zero-padded 3-digit dataset (e.g. `2#005`).
//! - `iptcembed` inserts the APP13 segment immediately after SOI and drops any
//!   pre-existing APP13, leaving all other segments and the entropy-coded data
//!   untouched, so the result stays a decodable JPEG.

use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

use crate::xfer::{in_bytes, set_out};
use crate::{cstr_arg, ffi_guard};

/// Datasets from the last `iptcparse`, grouped by `record#dataset` key in
/// first-seen order, each key holding its values in occurrence order. PHP's
/// `iptcparse` returns one array of values per key, so grouping here lets the
/// prelude build that shape with string-keyed writes only.
fn iptc_groups() -> &'static Mutex<Vec<(String, Vec<Vec<u8>>)>> {
    static CELL: OnceLock<Mutex<Vec<(String, Vec<Vec<u8>>)>>> = OnceLock::new();
    CELL.get_or_init(Mutex::default)
}

/// Parses the first `len` bytes of the input staging buffer as an IPTC IIM block,
/// groups the datasets by key, and returns the number of distinct keys, or `-1` if
/// the bytes are unavailable or contain no IPTC dataset (PHP's `iptcparse` returns
/// `false`).
#[no_mangle]
pub extern "C" fn elephc_iptc_parse(len: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(data) = in_bytes(len) else {
            return -1;
        };
        let Some(flat) = parse_iim(&data) else {
            return -1;
        };
        // Group by key, preserving first-seen key order and per-key value order.
        let mut groups: Vec<(String, Vec<Vec<u8>>)> = Vec::new();
        for (key, value) in flat {
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, values)) => values.push(value),
                None => groups.push((key, vec![value])),
            }
        }
        let count = groups.len() as i64;
        *iptc_groups().lock().unwrap() = groups;
        count
    })
}

/// Returns the number of distinct keys from the last `iptcparse`.
#[no_mangle]
pub extern "C" fn elephc_iptc_key_count() -> i64 {
    ffi_guard(-1, move || {
        iptc_groups().lock().unwrap().len() as i64
    })
}

/// Writes the key of group `index` to the shared output buffer and returns its
/// byte length, or `-1` if the index is out of range.
#[no_mangle]
pub extern "C" fn elephc_iptc_key(index: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = iptc_groups().lock().unwrap();
        let Some((key, _)) = usize::try_from(index).ok().and_then(|i| guard.get(i)) else {
            return -1;
        };
        let bytes = key.clone().into_bytes();
        drop(guard);
        set_out(bytes)
    })
}

/// Returns the number of values held under group `index`, or `-1` if the index is
/// out of range.
#[no_mangle]
pub extern "C" fn elephc_iptc_val_count(index: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = iptc_groups().lock().unwrap();
        match usize::try_from(index).ok().and_then(|i| guard.get(i)) {
            Some((_, values)) => values.len() as i64,
            None => -1,
        }
    })
}

/// Writes value `val_index` of group `key_index` to the shared output buffer and
/// returns its byte length, or `-1` if either index is out of range.
#[no_mangle]
pub extern "C" fn elephc_iptc_val(key_index: i64, val_index: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = iptc_groups().lock().unwrap();
        let value = usize::try_from(key_index)
            .ok()
            .and_then(|i| guard.get(i))
            .and_then(|(_, values)| usize::try_from(val_index).ok().and_then(|j| values.get(j)));
        let Some(value) = value else {
            return -1;
        };
        let bytes = value.clone();
        drop(guard);
        set_out(bytes)
    })
}

/// Walks an IPTC IIM block, returning each dataset as a `(record#dataset, value)`
/// pair, or `None` if no dataset could be read. Parsing stops at the first byte
/// that is not a `0x1C` tag marker or at a truncated record.
fn parse_iim(data: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut pos = 0usize;
    while pos + 5 <= data.len() {
        if data[pos] != 0x1C {
            break;
        }
        let record = data[pos + 1];
        let dataset = data[pos + 2];
        let len16 = ((data[pos + 3] as usize) << 8) | data[pos + 4] as usize;
        let mut p = pos + 5;
        let length = if len16 & 0x8000 != 0 {
            // Extended length: the low 15 bits count the bytes of the real length.
            let count = len16 & 0x7FFF;
            if count == 0 || count > 8 || p + count > data.len() {
                break;
            }
            let mut real = 0usize;
            for &byte in &data[p..p + count] {
                real = (real << 8) | byte as usize;
            }
            p += count;
            real
        } else {
            len16
        };
        if p + length > data.len() {
            break;
        }
        let key = format!("{record}#{dataset:03}");
        out.push((key, data[p..p + length].to_vec()));
        pos = p + length;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Reads the JPEG at `path`, embeds the IPTC data currently staged in the input
/// buffer (`in_len` bytes) as a Photoshop APP13 segment, writes the new JPEG to
/// the shared output buffer, and returns its byte length. Returns `-1` if the
/// data is unavailable, the file is unreadable or not a JPEG, or the IPTC block is
/// too large for a single APP13 segment.
#[no_mangle]
pub unsafe extern "C" fn elephc_iptc_embed(path: *const c_char, in_len: i64) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(iptc) = in_bytes(in_len) else {
            return -1;
        };
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let Ok(jpeg) = std::fs::read(path) else {
            return -1;
        };
        let Some(app13) = build_app13(&iptc) else {
            return -1;
        };
        let Some(out) = embed_app13(&jpeg, &app13) else {
            return -1;
        };
        set_out(out)
    })
}

/// Wraps an IPTC block in a Photoshop `8BIM` IPTC resource inside an APP13
/// segment, or `None` if the result would overflow the 16-bit segment length.
fn build_app13(iptc: &[u8]) -> Option<Vec<u8>> {
    // 8BIM resource: signature, resource id 0x0404 (IPTC), empty Pascal name
    // padded to an even length, 4-byte big-endian data size, then the data padded
    // to an even length.
    let mut resource: Vec<u8> = Vec::new();
    resource.extend_from_slice(b"8BIM");
    resource.extend_from_slice(&[0x04, 0x04]);
    resource.extend_from_slice(&[0x00, 0x00]);
    resource.extend_from_slice(&(iptc.len() as u32).to_be_bytes());
    resource.extend_from_slice(iptc);
    if iptc.len() % 2 == 1 {
        resource.push(0x00);
    }

    let mut payload: Vec<u8> = Vec::new();
    payload.extend_from_slice(b"Photoshop 3.0\x00");
    payload.extend_from_slice(&resource);

    let seg_len = payload.len() + 2;
    if seg_len > 0xFFFF {
        return None;
    }
    let mut seg: Vec<u8> = Vec::new();
    seg.extend_from_slice(&[0xFF, 0xED]);
    seg.extend_from_slice(&(seg_len as u16).to_be_bytes());
    seg.extend_from_slice(&payload);
    Some(seg)
}

/// Rebuilds a JPEG with `app13` inserted right after SOI and any pre-existing
/// APP13 removed. Returns `None` if the input does not start with the JPEG SOI.
fn embed_app13(jpeg: &[u8], app13: &[u8]) -> Option<Vec<u8>> {
    if jpeg.len() < 2 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut out: Vec<u8> = Vec::with_capacity(jpeg.len() + app13.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    out.extend_from_slice(app13);

    let mut pos = 2usize;
    loop {
        // A non-marker byte or a truncated header means the rest is data we copy
        // verbatim (this includes reaching the entropy-coded scan).
        if pos + 1 >= jpeg.len() || jpeg[pos] != 0xFF {
            out.extend_from_slice(&jpeg[pos..]);
            break;
        }
        let marker = jpeg[pos + 1];
        // Standalone markers carry no length payload.
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            out.extend_from_slice(&jpeg[pos..pos + 2]);
            pos += 2;
            continue;
        }
        // Start of scan: the compressed image data follows, copy everything left.
        if marker == 0xDA {
            out.extend_from_slice(&jpeg[pos..]);
            break;
        }
        if pos + 4 > jpeg.len() {
            out.extend_from_slice(&jpeg[pos..]);
            break;
        }
        let seg_len = ((jpeg[pos + 2] as usize) << 8) | jpeg[pos + 3] as usize;
        let seg_end = pos + 2 + seg_len;
        if seg_len < 2 || seg_end > jpeg.len() {
            out.extend_from_slice(&jpeg[pos..]);
            break;
        }
        // Drop an existing APP13 (PHP replaces it); copy every other segment.
        if marker != 0xED {
            out.extend_from_slice(&jpeg[pos..seg_end]);
        }
        pos = seg_end;
    }
    Some(out)
}
