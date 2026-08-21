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
//! - Binary transfer uses two per-thread byte cells instead of raw pointers across
//!   PHP-owned memory: the prelude resizes the *staging buffer* via
//!   `elephc_img_stage_ptr`, copies the PHP string into it with `ptr_write_string`,
//!   then asks the bridge to decode it; for output the bridge fills the *encode
//!   cell* and the prelude copies it out with `ptr_read_string`. elephc programs
//!   are single-threaded and each transfer is a synchronous fill→consume pair, so
//!   the returned pointers stay valid until the matching consume call ON THAT
//!   THREAD. The cells are per-thread because rewriting one reallocates it: a
//!   process-global cell would let one thread's `resize` free the buffer another
//!   thread is still writing into or reading from.
//! - JPEG honors a 0-100 quality (`-1` → 75); PNG is lossless (quality ignored),
//!   WebP is encoded lossless, and GIF/BMP take no quality. JPEG drops alpha by
//!   converting to RGB, matching the format.
//! - Every decoded image is stored true-color (GD decodes its formats to
//!   true-color images), so a GIF round-trip reports `imageistruecolor` = true
//!   here even though native GD GIFs are palette images.

use std::cell::RefCell;
use std::io::Cursor;
use std::os::raw::c_char;
use std::thread::LocalKey;

use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, RgbaImage};

use crate::{
    ffi_guard,
    lock_recover,
    cstr_arg, fmt_code_to_format, format_to_imagetype, images, insert_image, ImageObj, FMT_JPEG,
};

/// Per-thread staging buffer the prelude fills (via `ptr_write_string`) before
/// asking the bridge to decode it — the binary input counterpart of the encode
/// cell.
fn stage_cell() -> &'static LocalKey<RefCell<Vec<u8>>> {
    thread_local! {
        static STAGE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    &STAGE
}

/// Per-thread encode cell holding the most recently encoded image bytes, copied out
/// by the prelude (via `ptr_read_string`) for the no-file output path (PHP's
/// "write image to stdout").
fn encode_cell() -> &'static LocalKey<RefCell<Vec<u8>>> {
    thread_local! {
        static ENCODED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    &ENCODED
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

impl ProbeResult {
    /// The "nothing probed yet" value, spelled as a `const` so the per-thread cell
    /// can be initialized without a lazy-init check on every access.
    const ZERO: Self = Self {
        width: 0,
        height: 0,
        image_type: 0,
        bits: 0,
        channels: 0,
    };
}

/// Per-thread cell holding the last probe result, read back by the field
/// accessors.
fn probe_cell() -> &'static LocalKey<RefCell<ProbeResult>> {
    thread_local! {
        static CELL: RefCell<ProbeResult> = const { RefCell::new(ProbeResult::ZERO) };
    }
    &CELL
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
        stage_cell().with(|slot| {
            let mut slot = slot.borrow_mut();
            slot.clear();
            slot.resize(len as usize, 0);
            slot.as_mut_ptr()
        })
    })
}

/// Returns the `IMAGETYPE_*` code guessed from the first `len` staged bytes, or
/// `0` when the bytes are too short or the format is unrecognized. Used by the
/// Imagick bridge so `readImageBlob` can record the source format without a
/// second decode (the staging buffer still holds the bytes after a decode).
pub(crate) fn stage_guess_imagetype(len: usize) -> i64 {
    stage_cell().with(|slot| {
        let slot = slot.borrow();
        if len == 0 || slot.len() < len {
            return 0;
        }
        match image::guess_format(&slot[..len]) {
            Ok(format) => format_to_imagetype(format),
            Err(_) => 0,
        }
    })
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
        let len = len as usize;
        let decoded = stage_cell().with(|slot| {
            let slot = slot.borrow();
            if slot.len() < len {
                return None;
            }
            decode_bytes(&slot[..len], -1)
        });
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
        encode_cell().with(|slot| *slot.borrow_mut() = bytes);
        0
    })
}

/// Stores already-encoded bytes (e.g. a Cairo surface PNG) into the calling thread's
/// encode cell so the prelude can read them through `elephc_img_encoded_ptr`/`_len`
/// just like the GD/Imagick encode path. The previous contents are replaced.
pub(crate) fn set_encoded(bytes: Vec<u8>) {
    encode_cell().with(|slot| *slot.borrow_mut() = bytes);
}

/// Returns a read pointer to the encode cell's bytes. Valid until the next encode
/// or `elephc_img_encoded_clear`; the prelude reads it immediately after a
/// successful `elephc_img_encode`.
#[no_mangle]
pub extern "C" fn elephc_img_encoded_ptr() -> *const u8 {
    ffi_guard(std::ptr::null(), move || {
        encode_cell().with(|slot| slot.borrow().as_ptr())
    })
}

/// Returns the byte length of the encode cell.
#[no_mangle]
pub extern "C" fn elephc_img_encoded_len() -> i64 {
    ffi_guard(-1, move || {
        encode_cell().with(|slot| slot.borrow().len() as i64)
    })
}

/// Empties the encode cell, releasing its bytes once the prelude has copied them.
#[no_mangle]
pub extern "C" fn elephc_img_encoded_clear() {
    ffi_guard((), move || {
        encode_cell().with(|slot| slot.borrow_mut().clear());
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
        probe_cell().with(|slot| {
            *slot.borrow_mut() = ProbeResult {
                width: width as i64,
                height: height as i64,
                image_type: format_to_imagetype(format),
                bits: 8,
                channels,
            };
        });
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
        let len = len as usize;
        let Some((width, height, format, channels)) = stage_cell().with(|slot| {
            let slot = slot.borrow();
            if slot.len() < len {
                return None;
            }
            let reader = image::ImageReader::new(Cursor::new(&slot[..len]))
                .with_guessed_format()
                .ok()?;
            let format = reader.format()?;
            let channels = if format == ImageFormat::Jpeg { 3 } else { 4 };
            let (width, height) = reader.into_dimensions().ok()?;
            Some((width, height, format, channels))
        }) else {
            return -1;
        };
        probe_cell().with(|slot| {
            *slot.borrow_mut() = ProbeResult {
                width: width as i64,
                height: height as i64,
                image_type: format_to_imagetype(format),
                bits: 8,
                channels,
            };
        });
        0
    })
}

/// Returns the width from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_width() -> i64 {
    ffi_guard(-1, move || {
        probe_cell().with(|slot| slot.borrow().width)
    })
}

/// Returns the height from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_height() -> i64 {
    ffi_guard(-1, move || {
        probe_cell().with(|slot| slot.borrow().height)
    })
}

/// Returns the IMAGETYPE_* code from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_type() -> i64 {
    ffi_guard(-1, move || {
        probe_cell().with(|slot| slot.borrow().image_type)
    })
}

/// Returns the bit depth from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_bits() -> i64 {
    ffi_guard(-1, move || {
        probe_cell().with(|slot| slot.borrow().bits)
    })
}

/// Returns the channel count from the last successful probe.
#[no_mangle]
pub extern "C" fn elephc_img_probe_channels() -> i64 {
    ffi_guard(-1, move || {
        probe_cell().with(|slot| slot.borrow().channels)
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the cross-thread isolation of the byte cells this module hands
    //! pointers out of.
    //!
    //! Called from:
    //! - `cargo test -p elephc-image` through Rust's test harness.
    //!
    //! Key details:
    //! - The encode cell is the read-pointer path (`elephc_img_encoded_ptr`); the
    //!   staging cell is the write-pointer path (`elephc_img_stage_ptr`). Both are
    //!   per-thread for the same reason, and one guard covering the read path
    //!   demonstrates it without a test that writes through a stale pointer.

    use super::*;
    use crate::gd::elephc_img_create_truecolor;
    use crate::{images, FMT_PNG};

    /// Each thread's encoded bytes stay alive until that thread has copied them
    /// out. Two threads encoding *differently sized* images at the same moment
    /// must each read back their own: with one process-wide cell, the second
    /// thread's encode replaces the `Vec` the first was just handed a pointer
    /// into, freeing those bytes under it. (`elephc-pdo` shipped this exact bug
    /// and it reached CI as non-UTF-8 garbage where a SQLSTATE belonged.)
    ///
    /// A thread rewriting its OWN cell legitimately invalidates its own pointer —
    /// that is the documented contract — so each thread here encodes exactly once
    /// and the only interference is the other thread's.
    #[test]
    fn encode_buffers_are_isolated_between_threads() {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = [1_i64, 64_i64].map(|side| {
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                let handle = elephc_img_create_truecolor(side, side);
                assert!(handle >= 0, "image creation failed");

                // The expectation is computed straight from the image, never read
                // back out of the cell under test.
                let expected = {
                    let guard = lock_recover(images());
                    let obj = guard.get(&handle).expect("image handle");
                    encode_to_vec(obj, FMT_PNG, -1).expect("PNG encode")
                };

                barrier.wait();
                assert_eq!(elephc_img_encode(handle, FMT_PNG, -1), 0, "encode failed");
                let pointer = elephc_img_encoded_ptr();

                // Both threads have written their cell and taken their pointer
                // before either reads through it: that is the window in which one
                // process-wide cell frees the bytes the other thread points at.
                barrier.wait();
                assert_eq!(
                    elephc_img_encoded_len() as usize,
                    expected.len(),
                    "encode length was overwritten by the other thread"
                );
                let actual =
                    unsafe { std::slice::from_raw_parts(pointer, expected.len()) }.to_vec();
                assert_eq!(
                    actual, expected,
                    "encode buffer was overwritten by the other thread"
                );
            })
        });

        for handle in handles {
            handle.join().expect("encode worker must not panic");
        }
    }
}
