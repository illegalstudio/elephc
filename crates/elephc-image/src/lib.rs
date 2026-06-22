//! Purpose:
//! Pure-Rust image bridge for elephc's PHP image support (GD, Exif, Imagick,
//! Gmagick, Cairo). Exposes a small, stable C ABI (`elephc_img_*`) that the
//! elephc image prelude calls through `extern "elephc_image"` declarations.
//! Image objects are kept in a global handle table and referenced from PHP by
//! opaque `i64` IDs — raw pointers are never handed across the boundary, except
//! for the explicit blob-transfer primitives in `codec` (a staging buffer for
//! decode input and an encode cell for output), which exchange `(ptr, len)` with
//! the prelude's `ptr_read_string` / `ptr_write_string`.
//!
//! Called from:
//! - Compiled PHP programs that use any image symbol, via the elephc-PHP prelude
//!   (`src/image_prelude.rs`). `elephc_img_*` symbols are only referenced by
//!   image-using programs, so non-image binaries never link `-lelephc_image`.
//! - Built automatically by the compiler/linker (`cargo build -p elephc-image`).
//!
//! Key details:
//! - One global handle table indexes live images by `i64` IDs, guarded by a
//!   mutex. elephc programs are effectively single-threaded, so the mutex only
//!   manages contention without lock-free complexity.
//! - Each live image is an [`ImageObj`]: the RGBA pixel buffer plus GD-visible
//!   metadata (true-color vs palette flag, and the stored DPI resolution).
//! - Fallible entry points collapse failure to a `0`/`-1` sentinel; positive
//!   IDs are live handles.
//! - GD truecolor colors are packed `i64`s as `(alpha << 24) | (r << 16) |
//!   (g << 8) | b`, where `alpha` is GD's 7-bit value (0 = opaque … 127 =
//!   transparent).
//! - The C ABI surface is split across `gd` (raster + handle ops) and `codec`
//!   (file/blob I/O, encoding, and probing); this file owns the shared table,
//!   helpers, and format codes consumed by both.

mod cairo;
mod codec;
mod draw;
mod exif;
mod exif_tags;
mod fbuf;
mod filter;
mod gd;
mod imagick;
mod imagick_draw;
mod imagick_ops;
mod iptc;
mod text;
mod transform;
mod xfer;

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use image::{ImageFormat, Rgba, RgbaImage};

/// A live image in the handle table: the RGBA buffer plus GD-visible metadata.
pub(crate) struct ImageObj {
    /// The pixel buffer. Every elephc image is stored as 8-bit straight-alpha
    /// RGBA regardless of its origin format or GD palette/truecolor flag.
    pub(crate) img: RgbaImage,
    /// GD's true-color flag: `true` for `imagecreatetruecolor` and every decoded
    /// image, `false` for a palette image from `imagecreate`. Drives
    /// `imageistruecolor`.
    pub(crate) truecolor: bool,
    /// Horizontal resolution in DPI (GD default 96), reported by
    /// `imageresolution`.
    pub(crate) res_x: i64,
    /// Vertical resolution in DPI (GD default 96).
    pub(crate) res_y: i64,
    /// Alpha-blending mode (`imagealphablending`). When `true` (GD's true-color
    /// default), `imagesetpixel` composites the new color over the existing
    /// pixel; when `false` it overwrites, preserving the source alpha.
    pub(crate) alpha_blending: bool,
    /// Save-alpha flag (`imagesavealpha`). When `false` (GD default) the alpha
    /// channel is flattened to opaque on encode; when `true` it is preserved.
    pub(crate) save_alpha: bool,
    /// The GD packed transparent color (`imagecolortransparent`), or `-1` for
    /// none.
    pub(crate) transparent: i64,
    /// Line thickness in pixels (`imagesetthickness`, GD default 1), applied by
    /// the line/rectangle/polygon/arc outline primitives.
    pub(crate) thickness: i64,
    /// Pixel interpolation method (`imagesetinterpolation`, GD default
    /// `IMG_BILINEAR_FIXED` = 3), reported by `imagegetinterpolation`. The
    /// resize/scale path keys nearest-neighbour vs. linear sampling off this.
    pub(crate) interpolation: i64,
    /// Interlace / progressive-output flag (`imageinterlace`). Stored so the
    /// getter round-trips; the bundled encoders always emit non-interlaced output.
    pub(crate) interlace: bool,
}

impl ImageObj {
    /// Wraps a pixel buffer with GD's defaults: the given true-color flag, a
    /// 96-DPI resolution, alpha blending on, save-alpha off, and no transparent
    /// color — matching a freshly created GD true-color image.
    pub(crate) fn new(img: RgbaImage, truecolor: bool) -> Self {
        ImageObj {
            img,
            truecolor,
            res_x: 96,
            res_y: 96,
            alpha_blending: true,
            save_alpha: false,
            transparent: -1,
            thickness: 1,
            interpolation: 3,
            interlace: false,
        }
    }
}

/// Unpacks two 32-bit signed values packed into one `i64` as `(hi << 32) | lo`.
/// Several entry points pack coordinate/size pairs this way to stay within the
/// six-integer-argument limit of the x86_64 System V extern ABI (elephc does not
/// spill extern arguments onto the stack, so arguments past the sixth integer
/// register would be lost).
pub(crate) fn unpack_pair(packed: i64) -> (i64, i64) {
    let hi = (packed >> 32) as i32 as i64;
    let lo = packed as i32 as i64;
    (hi, lo)
}

/// Internal encoder-format codes shared by the prelude and the bridge. These are
/// the bridge's own selectors (passed to encode/write/decode-with-format) and are
/// distinct from PHP's `IMAGETYPE_*` values returned by `getimagesize`.
pub(crate) const FMT_PNG: i64 = 1;
pub(crate) const FMT_JPEG: i64 = 2;
pub(crate) const FMT_GIF: i64 = 3;
pub(crate) const FMT_BMP: i64 = 4;
pub(crate) const FMT_WEBP: i64 = 5;
pub(crate) const FMT_TGA: i64 = 6;

/// Global table of live images keyed by opaque handle ID.
pub(crate) fn images() -> &'static Mutex<HashMap<i64, ImageObj>> {
    static IMAGES: OnceLock<Mutex<HashMap<i64, ImageObj>>> = OnceLock::new();
    IMAGES.get_or_init(Mutex::default)
}

/// Returns a fresh, never-reused handle ID. IDs start at 1 so `0` and `-1`
/// remain available as "absent" / "error" sentinels.
pub(crate) fn next_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Inserts a new image into the handle table and returns its fresh handle ID.
pub(crate) fn insert_image(obj: ImageObj) -> i64 {
    let id = next_id();
    lock_recover(images()).insert(id, obj);
    id
}

/// Runs an FFI entry-point body, converting any panic into `fallback` so a panic
/// never unwinds across the C ABI boundary. On rustc ≥ 1.81 an unwinding panic out
/// of an `extern "C"` function aborts the process; catching it here instead lets the
/// bridge return PHP's failure sentinel (`false`/`-1`/null) and keep running, matching
/// PHP's "return false on error" semantics (mirrors `elephc-phar`). `AssertUnwindSafe`
/// is sound here: the bodies touch only process-global tables guarded by their own
/// `Mutex`es, and a poisoned lock after a caught panic degrades to further sentinels
/// rather than memory unsafety.
pub(crate) fn ffi_guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

/// Locks a process-global table, recovering the guard if a previously caught
/// panic poisoned the mutex. After [`ffi_guard`] swallows a panic that happened
/// while a table lock was held, the lock stays poisoned; the payload is still
/// structurally valid, so reusing it lets the bridge keep serving later calls
/// instead of degrading every subsequent operation to its failure sentinel.
pub(crate) fn lock_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Allocates a `width`×`height` RGBA image filled with `fill`, or returns `None`
/// when the request cannot be satisfied: non-positive dimensions, a `width*height*4`
/// byte count that overflows `usize`, or an allocation failure. Callers translate
/// `None` into PHP's failure sentinel (`false`/`-1`) instead of letting an oversized
/// request abort the process — `RgbaImage::from_pixel` would otherwise panic on a
/// capacity overflow or abort on allocation failure. `width`/`height` are `i64`
/// (the C ABI passes PHP ints) and validated to fit `u32` here.
pub(crate) fn try_new_rgba(width: i64, height: i64, fill: Rgba<u8>) -> Option<RgbaImage> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let w = u32::try_from(width).ok()?;
    let h = u32::try_from(height).ok()?;
    let bytes = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    let mut buf: Vec<u8> = Vec::new();
    buf.try_reserve_exact(bytes).ok()?;
    // Capacity is reserved above, so neither `resize` nor the pattern fill reallocates.
    buf.resize(bytes, 0);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&fill.0);
    }
    RgbaImage::from_raw(w, h, buf)
}

/// Borrows a C string argument as `&str`, returning `None` on null/!utf8.
pub(crate) unsafe fn cstr_arg<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// Maps an `image` crate format to PHP's `IMAGETYPE_*` integer code.
pub(crate) fn format_to_imagetype(format: ImageFormat) -> i64 {
    match format {
        ImageFormat::Gif => 1,
        ImageFormat::Jpeg => 2,
        ImageFormat::Png => 3,
        ImageFormat::Bmp => 6,
        ImageFormat::Tiff => 7,
        ImageFormat::WebP => 18,
        ImageFormat::Avif => 19,
        _ => 0,
    }
}

/// Maps an internal `FMT_*` encoder code to its `image` crate format, or `None`
/// for an unknown code.
pub(crate) fn fmt_code_to_format(fmt: i64) -> Option<ImageFormat> {
    match fmt {
        FMT_PNG => Some(ImageFormat::Png),
        FMT_JPEG => Some(ImageFormat::Jpeg),
        FMT_GIF => Some(ImageFormat::Gif),
        FMT_BMP => Some(ImageFormat::Bmp),
        FMT_WEBP => Some(ImageFormat::WebP),
        FMT_TGA => Some(ImageFormat::Tga),
        _ => None,
    }
}

/// Decodes a GD packed color (`(a7 << 24) | (r << 16) | (g << 8) | b`) into an
/// 8-bit RGBA pixel. GD's alpha is 7-bit with 0 = fully opaque and 127 = fully
/// transparent, which is rescaled to 8-bit straight alpha.
pub(crate) fn unpack_color(color: i64) -> Rgba<u8> {
    let c = color as u64;
    let r = ((c >> 16) & 0xff) as u8;
    let g = ((c >> 8) & 0xff) as u8;
    let b = (c & 0xff) as u8;
    let gd_alpha = ((c >> 24) & 0x7f) as u32;
    let a = (255 - (gd_alpha * 255 / 127)) as u8;
    Rgba([r, g, b, a])
}

/// Composites a straight-alpha source pixel over a destination pixel using the
/// standard source-over rule. Shared by `imagesetpixel` and the drawing
/// primitives when alpha blending is on.
pub(crate) fn blend_over(src: Rgba<u8>, dst: Rgba<u8>) -> Rgba<u8> {
    let sa = src.0[3] as u32;
    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let da = dst.0[3] as u32;
    let out_a = sa + da * (255 - sa) / 255;
    if out_a == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    let blend = |s: u8, d: u8| -> u8 {
        let s = s as u32;
        let d = d as u32;
        ((s * sa + d * da * (255 - sa) / 255) / out_a) as u8
    };
    Rgba([
        blend(src.0[0], dst.0[0]),
        blend(src.0[1], dst.0[1]),
        blend(src.0[2], dst.0[2]),
        out_a as u8,
    ])
}

/// Encodes an 8-bit RGBA pixel back into a GD packed color
/// (`(a7 << 24) | (r << 16) | (g << 8) | b`), the inverse of [`unpack_color`].
/// The 8-bit straight alpha is rescaled to GD's 7-bit value (255 → 0 opaque,
/// 0 → 127 transparent). Used by `imagecolorat`.
pub(crate) fn pack_color(pixel: Rgba<u8>) -> i64 {
    let [r, g, b, a] = pixel.0;
    let gd_alpha = ((255 - a as u32) * 127 / 255) as i64;
    (gd_alpha << 24) | ((r as i64) << 16) | ((g as i64) << 8) | b as i64
}
