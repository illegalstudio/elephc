//! Purpose:
//! Imagick per-image transforms, effects, and compositing. Geometry operations
//! (resize/scale/crop/rotate/flip/flop) delegate to the existing GD transform
//! entry points and swap the wand's current frame for the result; effects
//! (blur/negate/modulate/sharpen) and compositing mutate the frame's pixels
//! directly in the shared image table.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `Imagick::resizeImage`/`scaleImage`/`thumbnailImage`/`cropImage`/
//!   `rotateImage`/`flipImage`/`flopImage`/`blurImage`/`gaussianBlurImage`/
//!   `negateImage`/`modulateImage`/`sharpenImage`/`compositeImage`.
//!
//! Key details:
//! - Geometry ops reuse `elephc_img_scale`/`_crop`/`_rotate` (which allocate a new
//!   image) through [`crate::imagick::replace_current`], so Imagick inherits GD's
//!   sampling behavior exactly; `flipImage` (vertical) and `flopImage`
//!   (horizontal) map to the in-place `elephc_img_flip` modes.
//! - `modulateImage` works in HSL: brightness scales L, saturation scales S, and
//!   hue rotates H, with each percentage `100` meaning "unchanged" (matching
//!   Imagick's argument convention). This is a faithful reimplementation, not a
//!   byte-identical match to ImageMagick.
//! - `compositeImage` supports the two operators with a pure-Rust equivalent —
//!   `OVER` (source-over alpha blend) and `COPY` (overwrite) — and returns `-2`
//!   for any other operator so the prelude can raise `ImagickException`.

use image::{imageops, Rgba, RgbaImage};

use crate::filter::{convolve3x3, elephc_img_convolution, elephc_img_filter};
use crate::imagick::{current_handle, replace_current};
use crate::transform::{elephc_img_crop, elephc_img_flip, elephc_img_rotate, elephc_img_scale};
use crate::{ffi_guard, lock_recover, blend_over, images};

/// Imagick `COMPOSITE_OVER` operator code (also the default). Source-over blend.
const COMPOSITE_OVER: i64 = 40;
/// Imagick `COMPOSITE_COPY` operator code. Overwrites the destination region.
const COMPOSITE_COPY: i64 = 42;

/// Resizes the active frame to `cols`×`rows` with bilinear sampling, returning `0`
/// on success and `-1` on failure. Backs `Imagick::resizeImage`/`thumbnailImage`
/// (the prelude resolves best-fit dimensions before calling).
#[no_mangle]
pub extern "C" fn elephc_imagick_resize(wand_id: i64, cols: i64, rows: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        // mode 0 is not IMG_NEAREST_NEIGHBOUR (16), so elephc_img_scale uses bilinear.
        let new_handle = elephc_img_scale(handle, cols, rows, 0);
        replace_current(wand_id, new_handle)
    })
}

/// Scales the active frame to `cols`×`rows` with nearest-neighbour sampling,
/// returning `0`/`-1`. Backs `Imagick::scaleImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_scale(wand_id: i64, cols: i64, rows: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let new_handle = elephc_img_scale(handle, cols, rows, 16);
        replace_current(wand_id, new_handle)
    })
}

/// Crops a `width`×`height` rectangle at `(x, y)` from the active frame, returning
/// `0`/`-1`. Backs `Imagick::cropImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_crop(
    wand_id: i64,
    width: i64,
    height: i64,
    x: i64,
    y: i64,
) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let new_handle = elephc_img_crop(handle, x, y, width, height);
        replace_current(wand_id, new_handle)
    })
}

/// Rotates the active frame by `angle_mdeg` millidegrees (clockwise, Imagick
/// convention) over the GD packed `bg` color, returning `0`/`-1`. Backs
/// `Imagick::rotateImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_rotate(wand_id: i64, angle_mdeg: i64, bg: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        // GD/elephc rotate is counter-clockwise; Imagick is clockwise, so negate.
        let new_handle = elephc_img_rotate(handle, -angle_mdeg, bg);
        replace_current(wand_id, new_handle)
    })
}

/// Flips the active frame vertically (top↔bottom) in place, returning `0`/`-1`.
/// Backs `Imagick::flipImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_flip(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match current_handle(wand_id) {
            Some(handle) => elephc_img_flip(handle, 2),
            None => -1,
        }
    })
}

/// Flops the active frame horizontally (left↔right) in place, returning `0`/`-1`.
/// Backs `Imagick::flopImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_flop(wand_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match current_handle(wand_id) {
            Some(handle) => elephc_img_flip(handle, 1),
            None => -1,
        }
    })
}

/// Gaussian-blurs the active frame with the given `sigma` (16.16-free milli
/// units: `sigma_milli` = sigma × 1000), returning `0`/`-1`. Backs
/// `Imagick::blurImage` and `gaussianBlurImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_blur(wand_id: i64, sigma_milli: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let sigma = (sigma_milli as f32 / 1000.0).max(0.1);
        let mut guard = lock_recover(images());
        let Some(obj) = guard.get_mut(&handle) else {
            return -1;
        };
        obj.img = imageops::blur(&obj.img, sigma);
        0
    })
}

/// Negates (inverts) the active frame's RGB channels, returning `0`/`-1`. The
/// `only_gray` flag is accepted for API parity but treated as a full negate.
/// Backs `Imagick::negateImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_negate(wand_id: i64, only_gray: i64) -> i64 {
    ffi_guard(-1, move || {
        let _ = only_gray;
        match current_handle(wand_id) {
            Some(handle) => elephc_img_filter(handle, 0, 0, 0, 0, 0),
            None => -1,
        }
    })
}

/// Converts a straight-alpha RGB pixel to HSL (each component in `0.0..=1.0`).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-9 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == rf {
        (gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }
    } else if max == gf {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    } / 6.0;
    (h, s, l)
}

/// Converts an HSL triple (each in `0.0..=1.0`, hue wrapping) back to 8-bit RGB
/// using the standard piecewise hue-to-channel reconstruction.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s <= 1e-9 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    // Reconstructs one channel from the hue offset `t` (wrapped into 0..1).
    let conv = |t: f64| -> u8 {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (conv(h + 1.0 / 3.0), conv(h), conv(h - 1.0 / 3.0))
}

/// Adjusts brightness/saturation/hue of the active frame in HSL space, where each
/// of `brightness_pct`/`saturation_pct`/`hue_pct` is a percentage with `100`
/// meaning "unchanged". Returns `0`/`-1`. Backs `Imagick::modulateImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_modulate(
    wand_id: i64,
    brightness_pct: i64,
    saturation_pct: i64,
    hue_pct: i64,
) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let bf = brightness_pct as f64 / 100.0;
        let sf = saturation_pct as f64 / 100.0;
        // Imagick maps hue 0..200 to a -180..+180 degree rotation (100 = no change).
        let hue_shift = (hue_pct as f64 - 100.0) / 100.0; // in turns (1.0 = 360°)
        let mut guard = lock_recover(images());
        let Some(obj) = guard.get_mut(&handle) else {
            return -1;
        };
        for pixel in obj.img.pixels_mut() {
            let [r, g, b, a] = pixel.0;
            let (mut h, mut s, mut l) = rgb_to_hsl(r, g, b);
            h = (h + hue_shift).rem_euclid(1.0);
            s = (s * sf).clamp(0.0, 1.0);
            l = (l * bf).clamp(0.0, 1.0);
            let (nr, ng, nb) = hsl_to_rgb(h, s, l);
            *pixel = Rgba([nr, ng, nb, a]);
        }
        0
    })
}

/// Sharpens the active frame with a fixed 3×3 unsharp kernel, returning `0`/`-1`.
/// The `radius`/`sigma` arguments are accepted for API parity; the kernel is
/// fixed. Backs `Imagick::sharpenImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_sharpen(wand_id: i64, radius_milli: i64, sigma_milli: i64) -> i64 {
    ffi_guard(-1, move || {
        let _ = (radius_milli, sigma_milli);
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        let mut guard = lock_recover(images());
        let Some(obj) = guard.get_mut(&handle) else {
            return -1;
        };
        let orig = obj.img.clone();
        // 3×3 sharpen kernel (sum 1): center 5, edge-neighbours -1. Edge pixels
        // replicate (clamp), divisor 1, no offset — the shared convolution helper.
        let k = [0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0];
        convolve3x3(&mut obj.img, &orig, k, 1.0, 0.0);
        0
    })
}

/// Applies the 3×3 convolution kernel previously pushed via the `fbuf` buffer to
/// the active frame, dividing by `div_fixed` and adding `offset_fixed` (both
/// 16.16 fixed-point). Returns `0` on success and `-1` for an empty/unknown wand
/// or a wrong-length kernel. Backs `Imagick::convolveImage` with an
/// `ImagickKernel`.
#[no_mangle]
pub extern "C" fn elephc_imagick_convolve(wand_id: i64, div_fixed: i64, offset_fixed: i64) -> i64 {
    ffi_guard(-1, move || {
        match current_handle(wand_id) {
            Some(handle) => elephc_img_convolution(handle, div_fixed, offset_fixed),
            None => -1,
        }
    })
}

/// Composites the current frame of `src_wand` onto the current frame of
/// `dst_wand` at `(x, y)` using operator `op`. Returns `0` on success, `-1` for a
/// missing wand/frame, and `-2` for an unsupported operator. Backs
/// `Imagick::compositeImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_composite(
    dst_wand: i64,
    src_wand: i64,
    op: i64,
    x: i64,
    y: i64,
) -> i64 {
    ffi_guard(-1, move || {
        if op != COMPOSITE_OVER && op != COMPOSITE_COPY {
            return -2;
        }
        let Some(dst_handle) = current_handle(dst_wand) else {
            return -1;
        };
        let Some(src_handle) = current_handle(src_wand) else {
            return -1;
        };
        // Clone the source frame so a self-composite (same handle) cannot alias.
        let src_img: RgbaImage = {
            let guard = lock_recover(images());
            let Some(obj) = guard.get(&src_handle) else {
                return -1;
            };
            obj.img.clone()
        };
        let blend = op == COMPOSITE_OVER;
        let mut guard = lock_recover(images());
        let Some(dst) = guard.get_mut(&dst_handle) else {
            return -1;
        };
        for j in 0..src_img.height() {
            for i in 0..src_img.width() {
                let (tx, ty) = (x + i as i64, y + j as i64);
                if tx < 0 || ty < 0 || tx as u32 >= dst.img.width() || ty as u32 >= dst.img.height() {
                    continue;
                }
                let src_pixel = *src_img.get_pixel(i, j);
                let out = if blend {
                    blend_over(src_pixel, *dst.img.get_pixel(tx as u32, ty as u32))
                } else {
                    src_pixel
                };
                dst.img.put_pixel(tx as u32, ty as u32, out);
            }
        }
        0
    })
}
