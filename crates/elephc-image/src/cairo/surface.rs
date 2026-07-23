//! Purpose:
//! Image-surface (Pixmap) C ABI entry points for the Cairo bridge: create / destroy
//! surfaces, query their dimensions, encode or write them to PNG, and decode a PNG
//! file into a new surface. Each surface is a `tiny_skia::Pixmap` stored in the shared
//! surface table.
//!
//! Called from:
//! - the image prelude's `extern "elephc_image"` block (`CairoImageSurface`).
//!
//! Key details:
//! - PNG decode premultiplies straight alpha into the pixel buffer the way tiny-skia
//!   stores it. Encoding routes the bytes through the shared encode cell
//!   (`crate::codec::set_encoded`), read back via `elephc_img_encoded_ptr`/`_len`.

use std::ffi::c_char;

use image::ImageReader;
use tiny_skia::Pixmap;

use super::{next_id, surfaces};
use crate::codec::set_encoded;
use crate::{cstr_arg, ffi_guard, lock_recover};

/// Creates an RGBA8 image surface of the given size. Returns its handle, or -1.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_create(w: i64, h: i64) -> i64 {
    ffi_guard(-1, move || {
        if w <= 0 || h <= 0 {
            return -1;
        }
        let Some(pm) = Pixmap::new(w as u32, h as u32) else {
            return -1;
        };
        let id = next_id();
        lock_recover(surfaces()).insert(id, pm);
        id
    })
}

/// Destroys a surface, freeing its pixel buffer. Idempotent.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_destroy(s: i64) {
    ffi_guard((), move || {
        lock_recover(surfaces()).remove(&s);
    })
}

/// Returns the surface width in pixels, or -1 if the handle is unknown.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_width(s: i64) -> i64 {
    ffi_guard(-1, move || {
        lock_recover(surfaces())
            .get(&s)
            .map_or(-1, |pm| pm.width() as i64)
    })
}

/// Returns the surface height in pixels, or -1 if the handle is unknown.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_height(s: i64) -> i64 {
    ffi_guard(-1, move || {
        lock_recover(surfaces())
            .get(&s)
            .map_or(-1, |pm| pm.height() as i64)
    })
}

/// Encodes the surface as PNG into the shared encode cell, returning the byte
/// length (read back via `elephc_img_encoded_ptr`/`_len`), or -1 on failure.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_encode_png(s: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = lock_recover(surfaces());
        let Some(pm) = guard.get(&s) else {
            return -1;
        };
        match pm.encode_png() {
            Ok(bytes) => {
                let len = bytes.len() as i64;
                set_encoded(bytes);
                len
            }
            Err(_) => -1,
        }
    })
}

/// Writes the surface to a PNG file at `path`. Returns 0 on success, -1 on error.
///
/// # Safety
/// `path` must be a valid NUL-terminated C string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_cairo_surface_write_png(s: i64, path: *const c_char) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let guard = lock_recover(surfaces());
        let Some(pm) = guard.get(&s) else {
            return -1;
        };
        pm.save_png(path).map(|_| 0).unwrap_or(-1)
    })
}

/// Decodes a PNG file into a new image surface, premultiplying its alpha into the
/// pixel buffer the way tiny-skia's `Pixmap` stores it. Returns the surface handle,
/// or -1 if the file is missing/undecodable or the dimensions are invalid.
///
/// # Safety
/// `path` must be a valid NUL-terminated C string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_cairo_surface_create_from_png(path: *const c_char) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(path) = cstr_arg(path) else {
            return -1;
        };
        let Ok(reader) = ImageReader::open(path).and_then(|r| r.with_guessed_format()) else {
            return -1;
        };
        let Ok(dynimg) = reader.decode() else {
            return -1;
        };
        let rgba = dynimg.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let Some(mut pm) = Pixmap::new(w, h) else {
            return -1;
        };
        // tiny-skia stores premultiplied RGBA; the `image` crate yields straight alpha,
        // so multiply each channel by its alpha before copying it into the pixmap.
        let dst = pm.data_mut();
        let src = rgba.as_raw();
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            let a = s[3] as f32 / 255.0;
            d[0] = (s[0] as f32 * a).round().clamp(0.0, 255.0) as u8;
            d[1] = (s[1] as f32 * a).round().clamp(0.0, 255.0) as u8;
            d[2] = (s[2] as f32 * a).round().clamp(0.0, 255.0) as u8;
            d[3] = s[3];
        }
        let id = next_id();
        lock_recover(surfaces()).insert(id, pm);
        id
    })
}
