//! Purpose:
//! Imagick "wand" object table and its lifecycle / I/O / iteration surface. A
//! wand is a sequence of frames, each frame being a GD image handle in the shared
//! [`crate::images`] table, plus the wand's current-frame index, image format
//! code, and compression quality. Modeling frames as GD handles lets every
//! Imagick transform and effect reuse the existing `elephc_img_*` operations.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind the `Imagick` class methods (`readImage`, `newImage`, `writeImage`,
//!   `getImageBlob`, `getImageWidth`/`Height`, `getNumberImages`, iterator moves).
//! - `crate::imagick_ops` and `crate::imagick_draw`, which call the pub(crate)
//!   helpers [`current_handle`] / [`replace_current`] to operate on the current
//!   frame.
//!
//! Key details:
//! - The wand format is stored as an internal `FMT_*` code (the same selectors the
//!   encoder uses), not PHP's `IMAGETYPE_*`; `readImage`/`readImageBlob` map the
//!   detected `IMAGETYPE_*` to a `FMT_*` here so the rest of the bridge stays in
//!   one numbering. The PHP layer maps `FMT_*` to/from the format string.
//! - Destroying a wand frees every frame handle it owns; the GD destroy is
//!   idempotent, so an explicit `clear()`/`destroy()` plus the `__destruct` path
//!   cannot double-free.
//! - Fallible entry points use the `0`/`-1` sentinel convention; positive wand IDs
//!   are live handles, and IDs are drawn from a counter independent of the image
//!   table (lookups are table-specific, so the two ID spaces never collide).

use std::collections::HashMap;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use image::Rgba;

use crate::codec::{
    elephc_img_create_from_file, elephc_img_create_from_stage, elephc_img_encode,
    elephc_img_encoded_len, elephc_img_probe_file, elephc_img_probe_type, elephc_img_write_file,
    stage_guess_imagetype,
};
use crate::gd::{elephc_img_color_at, elephc_img_destroy, elephc_img_sx, elephc_img_sy};
use crate::{
    ffi_guard,
    lock_recover,
    cstr_arg, images, insert_image, try_new_rgba, unpack_color, ImageObj, FMT_BMP, FMT_GIF,
    FMT_JPEG, FMT_PNG, FMT_WEBP,
};

/// A live Imagick wand: an ordered list of frame handles (each a GD image in the
/// shared table), the current-frame index, the wand's `FMT_*` format code, and
/// the compression quality (`-1` = encoder default).
pub(crate) struct Wand {
    /// Frame handles into [`crate::images`], in sequence order.
    pub(crate) frames: Vec<i64>,
    /// Index of the active frame the per-image methods operate on.
    pub(crate) current: usize,
    /// Image format as an internal `FMT_*` code (PNG by default).
    pub(crate) format: i64,
    /// Compression quality 0-100, or `-1` for the encoder default.
    pub(crate) quality: i64,
}

/// Global table of live wands keyed by opaque wand ID.
pub(crate) fn wands() -> &'static Mutex<HashMap<i64, Wand>> {
    static WANDS: OnceLock<Mutex<HashMap<i64, Wand>>> = OnceLock::new();
    WANDS.get_or_init(Mutex::default)
}

/// Returns a fresh, never-reused wand ID. IDs start at 1 so `0`/`-1` stay free as
/// "absent" / "error" sentinels, independent of the image-handle counter.
fn next_wand_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Maps a detected `IMAGETYPE_*` code to the internal `FMT_*` encoder selector,
/// defaulting to PNG for formats the bridge cannot re-encode.
fn imagetype_to_fmt(image_type: i64) -> i64 {
    match image_type {
        1 => FMT_GIF,
        2 => FMT_JPEG,
        3 => FMT_PNG,
        6 => FMT_BMP,
        18 => FMT_WEBP,
        _ => FMT_PNG,
    }
}

/// Returns the active frame's GD image handle, or `None` for an unknown wand or a
/// wand with no frames. Shared with the ops/draw modules.
pub(crate) fn current_handle(wand_id: i64) -> Option<i64> {
    let guard = lock_recover(wands());
    let wand = guard.get(&wand_id)?;
    wand.frames.get(wand.current).copied()
}

/// Replaces the active frame's handle with `new_handle` (e.g. after a transform
/// that allocates a fresh image), destroying the previous frame. Returns `0` on
/// success and `-1` for an unknown wand or a negative `new_handle` (in which case
/// the old frame is left intact). Shared with the ops module.
pub(crate) fn replace_current(wand_id: i64, new_handle: i64) -> i64 {
    if new_handle < 0 {
        return -1;
    }
    let old = {
        let mut guard = lock_recover(wands());
        let Some(wand) = guard.get_mut(&wand_id) else {
            // No wand to attach the new image to: free it so it does not leak.
            drop(guard);
            elephc_img_destroy(new_handle);
            return -1;
        };
        let Some(slot) = wand.frames.get_mut(wand.current) else {
            drop(guard);
            elephc_img_destroy(new_handle);
            return -1;
        };
        let old = *slot;
        *slot = new_handle;
        old
    };
    elephc_img_destroy(old);
    0
}

/// Appends a frame handle to a wand, makes it current, and records its format.
/// Returns `0` on success and `-1` for an unknown wand (the orphaned handle is
/// freed so it does not leak).
fn append_frame(wand_id: i64, handle: i64, fmt: i64) -> i64 {
    let mut guard = lock_recover(wands());
    let Some(wand) = guard.get_mut(&wand_id) else {
        drop(guard);
        elephc_img_destroy(handle);
        return -1;
    };
    wand.frames.push(handle);
    wand.current = wand.frames.len() - 1;
    wand.format = fmt;
    0
}

/// Creates a new empty wand and returns its handle.
#[no_mangle]
pub extern "C" fn elephc_imagick_new() -> i64 {
    ffi_guard(-1, move || {
        let id = next_wand_id();
        lock_recover(wands()).insert(
            id,
            Wand {
                frames: Vec::new(),
                current: 0,
                format: FMT_PNG,
                quality: -1,
            },
        );
        id
    })
}

/// Destroys a wand, freeing every frame it owns and removing it from the table.
/// Idempotent for an unknown/already-destroyed wand. Backs `Imagick::destroy`
/// and the `__destruct` path.
#[no_mangle]
pub extern "C" fn elephc_imagick_destroy(wand_id: i64) {
    ffi_guard((), move || {
        let frames = lock_recover(wands()).remove(&wand_id).map(|w| w.frames);
        if let Some(frames) = frames {
            for handle in frames {
                elephc_img_destroy(handle);
            }
        }
    })
}

/// Frees every frame of a wand but keeps the (now empty) wand alive. Backs
/// `Imagick::clear`.
#[no_mangle]
pub extern "C" fn elephc_imagick_clear(wand_id: i64) {
    ffi_guard((), move || {
        let mut guard = lock_recover(wands());
        if let Some(wand) = guard.get_mut(&wand_id) {
            let frames = std::mem::take(&mut wand.frames);
            wand.current = 0;
            drop(guard);
            for handle in frames {
                elephc_img_destroy(handle);
            }
        }
    })
}

/// Returns the number of frames in a wand, or `-1` for an unknown wand. Backs
/// `Imagick::getNumberImages` and the `Countable` `count()`.
#[no_mangle]
pub extern "C" fn elephc_imagick_count(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(wands()).get(&wand_id) {
            Some(wand) => wand.frames.len() as i64,
            None => -1,
        }
    })
}

/// Reads an image file (auto-detecting the format) and appends it as a new frame,
/// recording the detected format. Returns `0` on success and `-1` if the file is
/// missing/unreadable/undecodable or the wand is unknown. Backs
/// `Imagick::readImage` / the path constructor.
#[no_mangle]
pub unsafe extern "C" fn elephc_imagick_read_file(wand_id: i64, path: *const c_char) -> i64 {
    ffi_guard(-1, move || unsafe {
        if cstr_arg(path).is_none() {
            return -1;
        }
        // Probe first so the detected IMAGETYPE is available for the format record,
        // then decode the pixels (auto-detect via expected_fmt = 0).
        let fmt = if elephc_img_probe_file(path) == 0 {
            imagetype_to_fmt(elephc_img_probe_type())
        } else {
            FMT_PNG
        };
        let handle = elephc_img_create_from_file(path, 0);
        if handle < 0 {
            return -1;
        }
        append_frame(wand_id, handle, fmt)
    })
}

/// Decodes the first `len` bytes of the shared staging buffer (filled by the
/// prelude via `ptr_write_string`) and appends the result as a new frame,
/// recording the detected format. Returns `0` on success and `-1` on a bad length
/// or undecodable bytes. Backs `Imagick::readImageBlob`.
#[no_mangle]
pub extern "C" fn elephc_imagick_read_blob(wand_id: i64, len: i64) -> i64 {
    ffi_guard(-1, move || {
        if len <= 0 {
            return -1;
        }
        let fmt = imagetype_to_fmt(stage_guess_imagetype(len as usize));
        let handle = elephc_img_create_from_stage(len);
        if handle < 0 {
            return -1;
        }
        append_frame(wand_id, handle, fmt)
    })
}

/// Creates a blank `width`×`height` frame filled with the GD packed `bg` color,
/// appends it, and records `fmt` (`FMT_*`). Returns `0` on success and `-1` for
/// invalid dimensions or an unknown wand. Backs `Imagick::newImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_new_image(
    wand_id: i64,
    width: i64,
    height: i64,
    bg: i64,
    fmt: i64,
) -> i64 {
    ffi_guard(-1, move || {
        let fill = unpack_color(bg);
        let Some(img) = try_new_rgba(width, height, fill) else {
            return -1;
        };
        let handle = insert_image(ImageObj::new(img, true));
        append_frame(wand_id, handle, if fmt > 0 { fmt } else { FMT_PNG })
    })
}

/// Clones the current frame of `src_wand` and appends it to `dst_wand`, carrying
/// the source format. Returns `0` on success and `-1` if either wand or the
/// source frame is missing. Backs `Imagick::addImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_add_image(dst_wand: i64, src_wand: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(src_handle) = current_handle(src_wand) else {
            return -1;
        };
        let src_fmt = wands()
            .lock()
            .unwrap()
            .get(&src_wand)
            .map(|w| w.format)
            .unwrap_or(FMT_PNG);
        let cloned = {
            let guard = lock_recover(images());
            let Some(obj) = guard.get(&src_handle) else {
                return -1;
            };
            let img = obj.img.clone();
            let truecolor = obj.truecolor;
            drop(guard);
            insert_image(ImageObj::new(img, truecolor))
        };
        append_frame(dst_wand, cloned, src_fmt)
    })
}

/// Returns the active frame's width in pixels, or `-1`. Backs
/// `Imagick::getImageWidth`.
#[no_mangle]
pub extern "C" fn elephc_imagick_cur_width(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match current_handle(wand_id) {
            Some(handle) => elephc_img_sx(handle),
            None => -1,
        }
    })
}

/// Returns the active frame's height in pixels, or `-1`. Backs
/// `Imagick::getImageHeight`.
#[no_mangle]
pub extern "C" fn elephc_imagick_cur_height(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match current_handle(wand_id) {
            Some(handle) => elephc_img_sy(handle),
            None => -1,
        }
    })
}

/// Sets the wand's `FMT_*` format code. Unknown wands are ignored. Backs
/// `Imagick::setImageFormat` (the PHP layer maps the format string to `FMT_*`).
#[no_mangle]
pub extern "C" fn elephc_imagick_set_format(wand_id: i64, fmt: i64) {
    ffi_guard((), move || {
        if let Some(wand) = lock_recover(wands()).get_mut(&wand_id) {
            wand.format = fmt;
        }
    })
}

/// Returns the wand's `FMT_*` format code, or `-1` for an unknown wand. Backs
/// `Imagick::getImageFormat`.
#[no_mangle]
pub extern "C" fn elephc_imagick_get_format(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(wands()).get(&wand_id) {
            Some(wand) => wand.format,
            None => -1,
        }
    })
}

/// Sets the wand's compression quality (0-100, or `-1` for default). Unknown
/// wands ignored. Backs `Imagick::setImageCompressionQuality`.
#[no_mangle]
pub extern "C" fn elephc_imagick_set_quality(wand_id: i64, quality: i64) {
    ffi_guard((), move || {
        if let Some(wand) = lock_recover(wands()).get_mut(&wand_id) {
            wand.quality = quality;
        }
    })
}

/// Returns the wand's compression quality, or `-1` for an unknown wand (also the
/// "default" sentinel). Backs `Imagick::getImageCompressionQuality`.
#[no_mangle]
pub extern "C" fn elephc_imagick_get_quality(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(wands()).get(&wand_id) {
            Some(wand) => wand.quality,
            None => -1,
        }
    })
}

/// Encodes the active frame to `path`. `fmt_override > 0` forces a format,
/// otherwise the wand's stored format is used. Returns `0` on success and `-1`
/// for an empty/unknown wand or a write/encode failure. Backs
/// `Imagick::writeImage`.
#[no_mangle]
pub unsafe extern "C" fn elephc_imagick_write_file(
    wand_id: i64,
    path: *const c_char,
    fmt_override: i64,
) -> i64 {
    ffi_guard(-1, move || unsafe {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let (fmt, quality) = {
            let guard = lock_recover(wands());
            let Some(wand) = guard.get(&wand_id) else {
                return -1;
            };
            let fmt = if fmt_override > 0 { fmt_override } else { wand.format };
            (fmt, wand.quality)
        };
        elephc_img_write_file(handle, fmt, path, quality)
    })
}

/// Encodes the active frame into the shared encode cell (read out by the prelude
/// via `elephc_img_encoded_ptr`/`_len`). `fmt_override > 0` forces a format.
/// Returns the encoded byte length on success and `-1` on failure. Backs
/// `Imagick::getImageBlob`.
#[no_mangle]
pub extern "C" fn elephc_imagick_get_blob(wand_id: i64, fmt_override: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let (fmt, quality) = {
            let guard = lock_recover(wands());
            let Some(wand) = guard.get(&wand_id) else {
                return -1;
            };
            let fmt = if fmt_override > 0 { fmt_override } else { wand.format };
            (fmt, wand.quality)
        };
        if elephc_img_encode(handle, fmt, quality) != 0 {
            return -1;
        }
        elephc_img_encoded_len()
    })
}

/// Returns the current iterator index, or `-1` for an unknown wand. Backs
/// `Imagick::getIteratorIndex` / `getImageIndex`.
#[no_mangle]
pub extern "C" fn elephc_imagick_get_index(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(wands()).get(&wand_id) {
            Some(wand) => wand.current as i64,
            None => -1,
        }
    })
}

/// Sets the current iterator index. Returns `0` on success and `-1` for an
/// unknown wand or an out-of-range index. Backs `Imagick::setIteratorIndex` /
/// `setImageIndex`.
#[no_mangle]
pub extern "C" fn elephc_imagick_set_index(wand_id: i64, index: i64) -> i64 {
    ffi_guard(-1, move || {
        let mut guard = lock_recover(wands());
        let Some(wand) = guard.get_mut(&wand_id) else {
            return -1;
        };
        if index < 0 || index as usize >= wand.frames.len() {
            return -1;
        }
        wand.current = index as usize;
        0
    })
}

/// Advances the iterator to the next frame. Returns `1` if it moved (there was a
/// next frame), `0` if already at the last frame, or `-1` for an unknown wand.
/// Backs `Imagick::nextImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_next(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        let mut guard = lock_recover(wands());
        let Some(wand) = guard.get_mut(&wand_id) else {
            return -1;
        };
        if wand.current + 1 < wand.frames.len() {
            wand.current += 1;
            1
        } else {
            0
        }
    })
}

/// Moves the iterator to the previous frame. Returns `1` if it moved, `0` if
/// already at the first frame, or `-1` for an unknown wand. Backs
/// `Imagick::previousImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_previous(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        let mut guard = lock_recover(wands());
        let Some(wand) = guard.get_mut(&wand_id) else {
            return -1;
        };
        if wand.current > 0 {
            wand.current -= 1;
            1
        } else {
            0
        }
    })
}

/// Resets the iterator to the first frame. Unknown wands ignored. Backs
/// `Imagick::setFirstIterator`.
#[no_mangle]
pub extern "C" fn elephc_imagick_first(wand_id: i64) {
    ffi_guard((), move || {
        if let Some(wand) = lock_recover(wands()).get_mut(&wand_id) {
            wand.current = 0;
        }
    })
}

/// Moves the iterator to the last frame. Unknown wands ignored. Backs
/// `Imagick::setLastIterator`.
#[no_mangle]
pub extern "C" fn elephc_imagick_last(wand_id: i64) {
    ffi_guard((), move || {
        if let Some(wand) = lock_recover(wands()).get_mut(&wand_id) {
            if !wand.frames.is_empty() {
                wand.current = wand.frames.len() - 1;
            }
        }
    })
}

/// Returns the GD packed color of a pixel in the active frame, or `-1` for an
/// unknown wand / out-of-bounds coordinate. Backs `Imagick::getImagePixelColor`
/// (the PHP layer wraps the result in an `ImagickPixel`).
#[no_mangle]
pub extern "C" fn elephc_imagick_pixel_color(wand_id: i64, x: i64, y: i64) -> i64 {
    ffi_guard(-1, move || {
        match current_handle(wand_id) {
            Some(handle) => elephc_img_color_at(handle, x, y),
            None => -1,
        }
    })
}

/// Fills the active frame entirely with the GD packed `color`. Returns `0` on
/// success and `-1` for an empty/unknown wand. Backs
/// `Imagick::setImageBackgroundColor` applied to an existing image.
#[no_mangle]
pub extern "C" fn elephc_imagick_fill(wand_id: i64, color: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let fill = unpack_color(color);
        let mut guard = lock_recover(images());
        let Some(obj) = guard.get_mut(&handle) else {
            return -1;
        };
        for pixel in obj.img.pixels_mut() {
            *pixel = Rgba(fill.0);
        }
        0
    })
}
