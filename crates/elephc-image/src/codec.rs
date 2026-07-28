//! Purpose:
//! Image codec and blob-transfer side of the bridge: decoding images from files
//! and in-memory bytes, encoding to files and to an in-memory cell, and probing a
//! file's dimensions/format for `getimagesize`. Supports the pure-Rust formats
//! `image` provides: PNG, JPEG, GIF, BMP, and WebP.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `imagecreatefrom{png,jpeg,gif,bmp,webp}`, `imagecreatefromstring`,
//!   the `image{png,jpeg,gif,bmp,webp}` output functions, and `getimagesize`.
//!
//! Key details:
//! - Binary transfer uses two static byte cells instead of raw pointers across
//!   PHP-owned memory: the prelude resizes the *staging buffer* via
//!   `elephc_img_stage_ptr`, copies the PHP string into it with `ptr_write_string`,
//!   then asks the bridge to decode it; for output the bridge fills the *encode
//!   cell* and the prelude copies it out with `ptr_read_string`. elephc programs
//!   are single-threaded and each transfer is a synchronous fill→consume pair, so
//!   the returned pointers stay valid until the matching consume call.
//! - JPEG honors a 0-100 quality (`-1` → 75); PNG is lossless (quality ignored),
//!   WebP is encoded lossless, and GIF/BMP take no quality. JPEG drops alpha by
//!   converting to RGB, matching the format.
//! - Every decoded image is stored true-color (GD decodes its formats to
//!   true-color images), so a GIF round-trip reports `imageistruecolor` = true
//!   here even though native GD GIFs are palette images.

use std::io::Cursor;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, RgbaImage};

use crate::{
    ffi_guard,
    lock_recover,
    cstr_arg, fmt_code_to_format, format_to_imagetype, images, insert_image, ImageObj, FMT_JPEG,
};

/// Staging buffer the prelude fills (via `ptr_write_string`) before asking the
/// bridge to decode it — the binary input counterpart of the encode cell.
fn stage_cell() -> &'static Mutex<Vec<u8>> {
    static STAGE: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    STAGE.get_or_init(Mutex::default)
}

/// Encode cell holding the most recently encoded image bytes, copied out by the
/// prelude (via `ptr_read_string`) for the no-file output path (PHP's
/// "write image to stdout").
fn encode_cell() -> &'static Mutex<Vec<u8>> {
    static ENCODED: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    ENCODED.get_or_init(Mutex::default)
}

/// Result of the most recent `getimagesize`-style probe: width, height,
/// IMAGETYPE_* code, bit depth, and channel count.
#[derive(Clone, Copy, Default)]
struct ProbeResult {
    width: i64,
    height: i64,
    image_type: i64,
    bits: i64,
    channels: i64,
}

/// Static cell holding the last probe result, read back by the field accessors.
fn probe_cell() -> &'static Mutex<ProbeResult> {
    static CELL: OnceLock<Mutex<ProbeResult>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(ProbeResult::default()))
}

/// Encodes an image to bytes in the requested format, returning `None` on an
/// unknown format code or an encoder error. JPEG uses the explicit quality
/// encoder over an RGB copy; the other formats round-trip RGBA through
/// `DynamicImage::write_to`.
fn encode_to_vec(obj: &ImageObj, fmt: i64, quality: i64) -> Option<Vec<u8>> {
    use image::codecs::jpeg::JpegEncoder;

    // GD discards the alpha channel on output unless imagesavealpha() is on; with
    // it off the image is written opaque. JPEG has no alpha regardless.
    let mut working = obj.img.clone();
    if !obj.save_alpha {
        for pixel in working.pixels_mut() {
            pixel.0[3] = 255;
        }
    }

    let mut buf: Vec<u8> = Vec::new();
    if fmt == FMT_JPEG {
        let rgb = DynamicImage::ImageRgba8(working).to_rgb8();
        let q = if quality < 0 {
            75
        } else {
            quality.clamp(0, 100) as u8
        };
        let mut cursor = Cursor::new(&mut buf);
        JpegEncoder::new_with_quality(&mut cursor, q)
            .write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
            .ok()?;
    } else {
        let format = fmt_code_to_format(fmt)?;
        let dynimg = DynamicImage::ImageRgba8(working);
        let mut cursor = Cursor::new(&mut buf);
        dynimg.write_to(&mut cursor, format).ok()?;
    }
    Some(buf)
}

/// Decodes image bytes to an RGBA buffer. With `expected_fmt > 0` the bytes must
/// match that format (so `imagecreatefrompng` rejects a JPEG); otherwise the
/// format is auto-detected (as `imagecreatefromstring` does).
fn decode_bytes(bytes: &[u8], expected_fmt: i64) -> Option<RgbaImage> {
    let dynimg = if expected_fmt > 0 {
        let format = fmt_code_to_format(expected_fmt)?;
        image::load_from_memory_with_format(bytes, format).ok()?
    } else {
        image::load_from_memory(bytes).ok()?
    };
    Some(dynimg.to_rgba8())
}

/// Resizes the staging buffer to `len` bytes (zero-filled) and returns a writable
/// pointer to its start, or null for a non-positive length. The prelude copies a
/// PHP string into this region with `ptr_write_string`, then calls
/// `elephc_img_create_from_stage`.
#[no_mangle]
pub extern "C" fn elephc_img_stage_ptr(len: i64) -> *mut u8 {
    ffi_guard(std::ptr::null_mut(), move || {
        if len <= 0 {
            return std::ptr::null_mut();
        }
        let Ok(len) = usize::try_from(len) else {
            return std::ptr::null_mut();
        };
        let mut guard = lock_recover(stage_cell());
        guard.clear();
        if guard.try_reserve_exact(len).is_err() {
            return std::ptr::null_mut();
        }
        guard.resize(len, 0);
        guard.as_mut_ptr()
    })
}

/// Returns the `IMAGETYPE_*` code guessed from the first `len` staged bytes, or
/// `0` when the bytes are too short or the format is unrecognized. Used by the
/// Imagick bridge so `readImageBlob` can record the source format without a
/// second decode (the staging buffer still holds the bytes after a decode).
pub(crate) fn stage_guess_imagetype(len: usize) -> i64 {
    let guard = lock_recover(stage_cell());
    if len == 0 || guard.len() < len {
        return 0;
    }
    match image::guess_format(&guard[..len]) {
        Ok(format) => format_to_imagetype(format),
        Err(_) => 0,
    }
}

/// Decodes the first `len` bytes of the staging buffer (auto-detecting the
/// format) into a new true-color image and returns its handle, or `-1` on a bad
/// length or undecodable bytes. Backs `imagecreatefromstring`.
#[no_mangle]
pub extern "C" fn elephc_img_create_from_stage(len: i64) -> i64 {
    ffi_guard(-1, move || {
        if len <= 0 {
            return -1;
        }
        let guard = lock_recover(stage_cell());
        let len = len as usize;
        if guard.len() < len {
            return -1;
        }
        let decoded = decode_bytes(&guard[..len], -1);
        drop(guard);
        match decoded {
            Some(img) => insert_image(ImageObj::new(img, true)),
            None => -1,
        }
    })
}

/// Decodes an image file into a new true-color image and returns its handle, or
/// `-1` if the file is missing/unreadable, undecodable, or (when
/// `expected_fmt > 0`) not of the required format. Backs the
/// `imagecreatefrom{png,jpeg,gif,bmp,webp,tga}` family.
#[no_mangle]
pub unsafe extern "C" fn elephc_img_create_from_file(
    path: *const c_char,
    expected_fmt: i64,
) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let Ok(mut reader) = image::ImageReader::open(path).and_then(|r| r.with_guessed_format()) else {
            return -1;
        };
        if expected_fmt > 0 {
            let Some(expected) = fmt_code_to_format(expected_fmt) else {
                return -1;
            };
            match reader.format() {
                // Sniffed format must match the requested one so imagecreatefrompng
                // rejects a JPEG.
                Some(guessed) if guessed != expected => return -1,
                // No sniffed format (TGA's header has no magic the sniffer recognizes)
                // or an exact match: pin the requested format and decode it.
                _ => reader.set_format(expected),
            }
        }
        let Ok(dynimg) = reader.decode() else {
            return -1;
        };
        insert_image(ImageObj::new(dynimg.to_rgba8(), true))
    })
}

/// Encodes an image to `path` in the given format. Returns `0` on success and
/// `-1` on an unknown handle/format, encode failure, or write error. Backs the
/// file form of `image{png,jpeg,gif,bmp,webp}`.
#[no_mangle]
pub unsafe extern "C" fn elephc_img_write_file(
    handle: i64,
    fmt: i64,
    path: *const c_char,
    quality: i64,
) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let guard = lock_recover(images());
        let Some(obj) = guard.get(&handle) else {
            return -1;
        };
        let Some(bytes) = encode_to_vec(obj, fmt, quality) else {
            return -1;
        };
        drop(guard);
        match std::fs::write(path, bytes) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    })
}

/// Encodes an image into the encode cell. Returns `0` on success and `-1` on an
/// unknown handle/format or encode failure. The prelude then reads
/// `elephc_img_encoded_len` / `elephc_img_encoded_ptr` and copies the bytes out
/// for the no-file output path.
#[no_mangle]
pub extern "C" fn elephc_img_encode(handle: i64, fmt: i64, quality: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = lock_recover(images());
        let Some(obj) = guard.get(&handle) else {
            return -1;
        };
        let Some(bytes) = encode_to_vec(obj, fmt, quality) else {
            return -1;
        };
        drop(guard);
        *lock_recover(encode_cell()) = bytes;
        0
    })
}

/// Stores already-encoded bytes (e.g. a Cairo surface PNG) into the shared encode
/// cell so the prelude can read them through `elephc_img_encoded_ptr`/`_len` just
/// like the GD/Imagick encode path. The previous contents are replaced.
pub(crate) fn set_encoded(bytes: Vec<u8>) {
    *lock_recover(encode_cell()) = bytes;
}

/// Returns a read pointer to the encode cell's bytes. Valid until the next encode
/// or `elephc_img_encoded_clear`; the prelude reads it immediately after a
/// successful `elephc_img_encode`.
#[no_mangle]
pub extern "C" fn elephc_img_encoded_ptr() -> *const u8 {
    ffi_guard(std::ptr::null(), move || {
        lock_recover(encode_cell()).as_ptr()
    })
}

/// Returns the byte length of the encode cell.
#[no_mangle]
pub extern "C" fn elephc_img_encoded_len() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(encode_cell()).len() as i64
    })
}

/// Empties the encode cell, releasing its bytes once the prelude has copied them.
#[no_mangle]
pub extern "C" fn elephc_img_encoded_clear() {
    ffi_guard((), move || {
        lock_recover(encode_cell()).clear();
    })
}

/// Probes an image file for its dimensions and format without fully decoding it,
/// storing the result in the static probe cell. Returns `0` on success and `-1`
/// if the file is missing/unreadable or its format is unrecognized.
#[no_mangle]
pub unsafe extern "C" fn elephc_img_probe_file(path: *const c_char) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let Ok(reader) = image::ImageReader::open(path).and_then(|r| r.with_guessed_format()) else {
            return -1;
        };
        let Some(format) = reader.format() else {
            return -1;
        };
        let channels = if format == ImageFormat::Jpeg { 3 } else { 4 };
        let Ok((width, height)) = reader.into_dimensions() else {
            return -1;
        };
        *lock_recover(probe_cell()) = ProbeResult {
            width: width as i64,
            height: height as i64,
            image_type: format_to_imagetype(format),
            bits: 8,
            channels,
        };
        0
    })
}

/// Probes the staging buffer — the bytes the prelude staged via
/// `elephc_img_stage_ptr` + `ptr_write_string` — for its dimensions and format
/// without fully decoding it, storing the result in the static probe cell. Returns
/// `0` on success and `-1` if the bytes are too short or their format is
/// unrecognized. Backs `getimagesizefromstring`.
#[no_mangle]
pub extern "C" fn elephc_img_probe_stage(len: i64) -> i64 {
    ffi_guard(-1, move || {
        if len <= 0 {
            return -1;
        }
        let guard = lock_recover(stage_cell());
        let len = len as usize;
        if guard.len() < len {
            return -1;
        }
        let Ok(reader) = image::ImageReader::new(Cursor::new(&guard[..len])).with_guessed_format() else {
            return -1;
        };
        let Some(format) = reader.format() else {
            return -1;
        };
        let channels = if format == ImageFormat::Jpeg { 3 } else { 4 };
        let Ok((width, height)) = reader.into_dimensions() else {
            return -1;
        };
        drop(guard);
        *lock_recover(probe_cell()) = ProbeResult {
            width: width as i64,
            height: height as i64,
            image_type: format_to_imagetype(format),
            bits: 8,
            channels,
        };
        0
    })
}

/// Returns the width from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_width() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(probe_cell()).width
    })
}

/// Returns the height from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_height() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(probe_cell()).height
    })
}

/// Returns the IMAGETYPE_* code from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_type() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(probe_cell()).image_type
    })
}

/// Returns the bit depth from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_bits() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(probe_cell()).bits
    })
}

/// Returns the channel count from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_channels() -> i64 {
    ffi_guard(-1, move || {
        lock_recover(probe_cell()).channels
    })
}
