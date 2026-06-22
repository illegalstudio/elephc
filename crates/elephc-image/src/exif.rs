//! Purpose:
//! EXIF metadata side of the bridge: parse a file's EXIF attributes into a
//! `key => value` list, look up a tag mnemonic, and extract the embedded
//! thumbnail. Backs PHP's `exif_read_data` / `read_exif_data`, `exif_tagname`,
//! and `exif_thumbnail`.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`.
//!   `elephc_exif_read` fills the shared key/value list (read out through
//!   `crate::xfer`); `elephc_exif_tagname` and the thumbnail entry points use the
//!   shared output buffer.
//!
//! Key details:
//! - Parsing uses the pure-Rust `kamadak-exif` crate. Field values are rendered to
//!   strings here (not via kamadak's interpretive `display_value`) so they stay
//!   close to PHP's raw forms: ASCII → text, SHORT/LONG → the integer(s),
//!   RATIONAL → `num/den`. This is a documented simplification — PHP returns typed
//!   scalars/arrays, elephc returns their string rendering.
//! - The thumbnail offset stored in IFD1 is relative to the TIFF header, which in
//!   a JPEG begins just after the `Exif\0\0` APP1 prefix; the extractor locates
//!   that prefix to rebase the offset, then slices the original bytes.

use std::io::Cursor;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

use exif::{In, Value};

use crate::exif_tags::php_tag_name;
use crate::xfer::{set_kv, set_out};
use crate::{ffi_guard, cstr_arg, format_to_imagetype};

/// Dimensions and IMAGETYPE code of the most recently extracted thumbnail.
#[derive(Clone, Copy, Default)]
struct ThumbMeta {
    width: i64,
    height: i64,
    image_type: i64,
}

/// Static cell holding the last thumbnail's metadata, read back by the accessors.
fn thumb_meta() -> &'static Mutex<ThumbMeta> {
    static CELL: OnceLock<Mutex<ThumbMeta>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(ThumbMeta::default()))
}

/// Renders one rational `num/den` component as PHP does.
fn fmt_rational(num: impl std::fmt::Display, den: impl std::fmt::Display) -> String {
    format!("{num}/{den}")
}

/// Renders an EXIF field value to a PHP-style string: ASCII/Undefined as text,
/// integer types as space-joined decimals, rationals as `num/den`, floats as
/// decimals. Multi-component values are space-joined.
fn render_value(value: &Value) -> String {
    match value {
        Value::Ascii(parts) => parts
            .iter()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect::<Vec<_>>()
            .join("")
            .trim_end_matches(['\0', ' '])
            .to_string(),
        Value::Byte(v) => join_nums(v),
        Value::SByte(v) => join_nums(v),
        Value::Short(v) => join_nums(v),
        Value::SShort(v) => join_nums(v),
        Value::Long(v) => join_nums(v),
        Value::SLong(v) => join_nums(v),
        Value::Float(v) => join_nums(v),
        Value::Double(v) => join_nums(v),
        Value::Rational(v) => v
            .iter()
            .map(|r| fmt_rational(r.num, r.denom))
            .collect::<Vec<_>>()
            .join(" "),
        Value::SRational(v) => v
            .iter()
            .map(|r| fmt_rational(r.num, r.denom))
            .collect::<Vec<_>>()
            .join(" "),
        Value::Undefined(bytes, _) => String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_string(),
        Value::Unknown(..) => String::new(),
    }
}

/// Space-joins a slice of `Display` numbers into one string (a single element
/// renders as just that number).
fn join_nums<T: std::fmt::Display>(values: &[T]) -> String {
    values
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a whole file into memory, or `None` if it cannot be read.
fn read_file(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Parses the EXIF attributes of `path` into the shared `key => value` list and
/// returns the field count, or `-1` if the file is unreadable or has no EXIF.
/// Each key is the PHP `exif_read_data` mnemonic; each value is the string
/// rendering of the field.
#[no_mangle]
pub unsafe extern "C" fn elephc_exif_read(path: *const c_char) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let Some(bytes) = read_file(path) else {
            return -1;
        };
        let reader = exif::Reader::new();
        let Ok(exif) = reader.read_from_container(&mut Cursor::new(&bytes)) else {
            return -1;
        };
        let mut list: Vec<(String, Vec<u8>)> = Vec::new();
        for field in exif.fields() {
            let key = php_tag_name(field.tag.context(), field.tag.number());
            let val = render_value(&field.value).into_bytes();
            list.push((key, val));
        }
        set_kv(list)
    })
}

/// Looks up the `exif_tagname` mnemonic for a tag number, writing it to the shared
/// output buffer and returning its byte length, or `-1` for an unknown tag (PHP's
/// `exif_tagname` returns `false`).
#[no_mangle]
pub extern "C" fn elephc_exif_tagname(number: i64) -> i64 {
    ffi_guard(-1, move || {
        let Ok(number) = u16::try_from(number) else {
            return -1;
        };
        match crate::exif_tags::tagname_default(number) {
            Some(name) => set_out(name.as_bytes().to_vec()),
            None => -1,
        }
    })
}

/// Extracts the embedded EXIF thumbnail from `path` into the shared output buffer,
/// records its dimensions/type, and returns the thumbnail byte length, or `-1` if
/// there is no thumbnail (or it cannot be located/decoded). Only JPEG-compressed
/// thumbnails are supported; uncompressed-TIFF thumbnails are a documented gap.
#[no_mangle]
pub unsafe extern "C" fn elephc_exif_thumbnail(path: *const c_char) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let Some(bytes) = read_file(path) else {
            return -1;
        };
        let Some((thumb, meta)) = extract_thumbnail(&bytes) else {
            return -1;
        };
        *thumb_meta().lock().unwrap() = meta;
        set_out(thumb)
    })
}

/// Parses EXIF, locates IFD1's JPEG thumbnail offset/length, rebases the offset
/// onto the TIFF header position inside the file, and returns the thumbnail bytes
/// plus decoded dimensions. Returns `None` if any step fails.
fn extract_thumbnail(bytes: &[u8]) -> Option<(Vec<u8>, ThumbMeta)> {
    let reader = exif::Reader::new();
    let exif = reader.read_from_container(&mut Cursor::new(bytes)).ok()?;

    let mut offset: Option<u32> = None;
    let mut length: Option<u32> = None;
    for field in exif.fields() {
        if field.ifd_num != In::THUMBNAIL {
            continue;
        }
        match field.tag.number() {
            0x0201 => offset = field.value.get_uint(0),
            0x0202 => length = field.value.get_uint(0),
            _ => {}
        }
    }
    let (offset, length) = (offset? as usize, length? as usize);
    if length == 0 {
        return None;
    }

    // The IFD1 offset is relative to the TIFF header, which in a JPEG sits right
    // after the "Exif\0\0" APP1 prefix; for a bare TIFF the header is at byte 0.
    let tiff_base = find_subslice(bytes, b"Exif\x00\x00").map(|p| p + 6).unwrap_or(0);
    let start = tiff_base.checked_add(offset)?;
    let end = start.checked_add(length)?;
    let thumb = bytes.get(start..end)?.to_vec();

    let image_type = match image::guess_format(&thumb) {
        Ok(fmt) => format_to_imagetype(fmt),
        Err(_) => 0,
    };
    let (width, height) = match image::load_from_memory(&thumb) {
        Ok(img) => (img.width() as i64, img.height() as i64),
        Err(_) => (0, 0),
    };
    Some((
        thumb,
        ThumbMeta {
            width,
            height,
            image_type,
        },
    ))
}

/// Returns the first index at which `needle` occurs in `haystack`, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Returns the width of the most recently extracted thumbnail.
#[no_mangle]
pub extern "C" fn elephc_exif_thumb_width() -> i64 {
    ffi_guard(-1, move || {
        thumb_meta().lock().unwrap().width
    })
}

/// Returns the height of the most recently extracted thumbnail.
#[no_mangle]
pub extern "C" fn elephc_exif_thumb_height() -> i64 {
    ffi_guard(-1, move || {
        thumb_meta().lock().unwrap().height
    })
}

/// Returns the IMAGETYPE code of the most recently extracted thumbnail.
#[no_mangle]
pub extern "C" fn elephc_exif_thumb_type() -> i64 {
    ffi_guard(-1, move || {
        thumb_meta().lock().unwrap().image_type
    })
}
