//! Purpose:
//! GD pixel-filter operations of the image bridge: the `imagefilter` family (all
//! `IMG_FILTER_*` selectors), arbitrary 3×3 `imageconvolution`, `imagegammacorrect`,
//! and the interpolation / interlace state accessors. Every operation mutates the
//! image in place; the convolution-based filters read from a clone so neighbor
//! sampling sees the original pixels.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `imagefilter`, `imageconvolution`, `imagegammacorrect`,
//!   `imagesetinterpolation`/`imagegetinterpolation`, and `imageinterlace`.
//!
//! Key details:
//! - The 3×3 convolution kernels and divisors match libgd's built-in filters
//!   (edge-detect, emboss, mean-removal, gaussian-blur, smooth); selective-blur is
//!   approximated by the gaussian kernel and documented as such.
//! - Convolution edge pixels clamp their sample coordinates to the image bounds
//!   (edge replication) and the alpha channel is preserved, matching GD.
//! - `imagefilter` matrices and `imageconvolution`'s kernel arrive as f64 through
//!   the [`crate::fbuf`] fixed-point buffer; scalar filter arguments arrive as
//!   plain ints. Scatter uses a fixed-seed LCG so its output is deterministic and
//!   testable (GD's scatter is randomized).

use image::{Rgba, RgbaImage};

use crate::{fbuf, images};

/// Built-in 3×3 convolution kernels keyed by `IMG_FILTER_*` selector, paired with
/// their divisor. Matches libgd's `gd_filter.c` definitions.
fn builtin_kernel(filter: i64, weight: f64) -> Option<([f64; 9], f64)> {
    match filter {
        // IMG_FILTER_EDGEDETECT
        5 => Some(([-1.0, 0.0, -1.0, 0.0, 4.0, 0.0, -1.0, 0.0, -1.0], 1.0)),
        // IMG_FILTER_EMBOSS
        6 => Some(([1.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.5], 1.0)),
        // IMG_FILTER_GAUSSIAN_BLUR and IMG_FILTER_SELECTIVE_BLUR (approximated)
        7 | 8 => Some(([1.0, 2.0, 1.0, 2.0, 4.0, 2.0, 1.0, 2.0, 1.0], 16.0)),
        // IMG_FILTER_MEAN_REMOVAL
        9 => Some(([-1.0, -1.0, -1.0, -1.0, 9.0, -1.0, -1.0, -1.0, -1.0], 1.0)),
        // IMG_FILTER_SMOOTH (center weight from arg1, divisor weight + 8)
        10 => {
            let div = weight + 8.0;
            Some((
                [1.0, 1.0, 1.0, 1.0, weight, 1.0, 1.0, 1.0, 1.0],
                if div == 0.0 { 1.0 } else { div },
            ))
        }
        _ => None,
    }
}

/// Clamps a coordinate to `[0, max)` for edge replication during convolution.
fn clamp_coord(v: i64, max: u32) -> u32 {
    v.clamp(0, max as i64 - 1) as u32
}

/// Applies a 3×3 convolution to the RGB channels in place, reading neighbors from
/// `orig` (a pre-convolution clone) with edge replication, dividing by `div` and
/// adding `offset`. Alpha is preserved.
fn convolve3x3(img: &mut RgbaImage, orig: &RgbaImage, k: [f64; 9], div: f64, offset: f64) {
    let (w, h) = (img.width(), img.height());
    let div = if div == 0.0 { 1.0 } else { div };
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f64; 3];
            for ky in 0..3 {
                for kx in 0..3 {
                    let px = clamp_coord(x as i64 + kx as i64 - 1, w);
                    let py = clamp_coord(y as i64 + ky as i64 - 1, h);
                    let s = orig.get_pixel(px, py).0;
                    let coef = k[ky * 3 + kx];
                    acc[0] += s[0] as f64 * coef;
                    acc[1] += s[1] as f64 * coef;
                    acc[2] += s[2] as f64 * coef;
                }
            }
            let a = img.get_pixel(x, y).0[3];
            let ch = |v: f64| (v / div + offset).round().clamp(0.0, 255.0) as u8;
            img.put_pixel(x, y, Rgba([ch(acc[0]), ch(acc[1]), ch(acc[2]), a]));
        }
    }
}

/// Inverts the RGB channels (`IMG_FILTER_NEGATE`), preserving alpha.
fn negate(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        p.0[0] = 255 - p.0[0];
        p.0[1] = 255 - p.0[1];
        p.0[2] = 255 - p.0[2];
    }
}

/// Converts to grayscale by luminance (`IMG_FILTER_GRAYSCALE`), preserving alpha.
fn grayscale(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let lum = (0.299 * p.0[0] as f64 + 0.587 * p.0[1] as f64 + 0.114 * p.0[2] as f64)
            .round()
            .clamp(0.0, 255.0) as u8;
        p.0[0] = lum;
        p.0[1] = lum;
        p.0[2] = lum;
    }
}

/// Adds `amount` to each RGB channel (`IMG_FILTER_BRIGHTNESS`), clamped.
fn brightness(img: &mut RgbaImage, amount: i64) {
    let adj = |c: u8| (c as i64 + amount).clamp(0, 255) as u8;
    for p in img.pixels_mut() {
        p.0[0] = adj(p.0[0]);
        p.0[1] = adj(p.0[1]);
        p.0[2] = adj(p.0[2]);
    }
}

/// Adjusts contrast about mid-gray (`IMG_FILTER_CONTRAST`), using libgd's
/// `((100 - level) / 100)^2` factor.
fn contrast(img: &mut RgbaImage, level: i64) {
    let adj = ((100.0 - level as f64) / 100.0).powi(2);
    let ch = |c: u8| (((c as f64 / 255.0 - 0.5) * adj + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
    for p in img.pixels_mut() {
        p.0[0] = ch(p.0[0]);
        p.0[1] = ch(p.0[1]);
        p.0[2] = ch(p.0[2]);
    }
}

/// Adds per-channel offsets (`IMG_FILTER_COLORIZE`): `r`/`g`/`b` shift the color
/// channels, and `a` shifts GD's 7-bit alpha (0 opaque … 127 transparent).
fn colorize(img: &mut RgbaImage, r: i64, g: i64, b: i64, a: i64) {
    for p in img.pixels_mut() {
        p.0[0] = (p.0[0] as i64 + r).clamp(0, 255) as u8;
        p.0[1] = (p.0[1] as i64 + g).clamp(0, 255) as u8;
        p.0[2] = (p.0[2] as i64 + b).clamp(0, 255) as u8;
        if a != 0 {
            let gd_alpha = ((255 - p.0[3] as i64) * 127 / 255 + a).clamp(0, 127);
            p.0[3] = (255 - gd_alpha * 255 / 127) as u8;
        }
    }
}

/// Pixelates in `block`×`block` cells (`IMG_FILTER_PIXELATE`). `advanced` = false
/// uses the block's top-left pixel; true uses the block average.
fn pixelate(img: &mut RgbaImage, block: i64, advanced: bool) -> bool {
    if block <= 0 {
        return false;
    }
    let block = block as u32;
    let (w, h) = (img.width(), img.height());
    let mut by = 0;
    while by < h {
        let mut bx = 0;
        while bx < w {
            let (ex, ey) = ((bx + block).min(w), (by + block).min(h));
            let color = if advanced {
                let mut sum = [0u64; 4];
                let mut n = 0u64;
                for y in by..ey {
                    for x in bx..ex {
                        let s = img.get_pixel(x, y).0;
                        for c in 0..4 {
                            sum[c] += s[c] as u64;
                        }
                        n += 1;
                    }
                }
                let n = n.max(1);
                Rgba([
                    (sum[0] / n) as u8,
                    (sum[1] / n) as u8,
                    (sum[2] / n) as u8,
                    (sum[3] / n) as u8,
                ])
            } else {
                *img.get_pixel(bx, by)
            };
            for y in by..ey {
                for x in bx..ex {
                    img.put_pixel(x, y, color);
                }
            }
            bx += block;
        }
        by += block;
    }
    true
}

/// Scatters pixels by swapping each with a neighbor offset in `[sub, plus]` per
/// axis (`IMG_FILTER_SCATTER`). A fixed-seed LCG keeps the result deterministic
/// (GD's scatter is randomized and not reproducible).
fn scatter(img: &mut RgbaImage, sub: i64, plus: i64) -> bool {
    let span = plus - sub;
    if span <= 0 {
        return false;
    }
    let (w, h) = (img.width() as i64, img.height() as i64);
    let mut state: u64 = 0x2545F491_4F6CDD1D;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) as i64
    };
    for y in 0..h {
        for x in 0..w {
            let ox = sub + next().rem_euclid(span + 1);
            let oy = sub + next().rem_euclid(span + 1);
            let (nx, ny) = (x + ox, y + oy);
            if nx >= 0 && ny >= 0 && nx < w && ny < h {
                let a = *img.get_pixel(x as u32, y as u32);
                let b = *img.get_pixel(nx as u32, ny as u32);
                img.put_pixel(x as u32, y as u32, b);
                img.put_pixel(nx as u32, ny as u32, a);
            }
        }
    }
    true
}

/// Applies an `IMG_FILTER_*` operation in place. Returns `0` on success, `-1` for
/// an unknown handle, unknown selector, or an invalid argument (e.g. a non-positive
/// pixelate block). `a1`-`a4` carry the selector's scalar arguments.
#[no_mangle]
pub extern "C" fn elephc_img_filter(handle: i64, filter: i64, a1: i64, a2: i64, a3: i64, a4: i64) -> i64 {
    let mut guard = images().lock().unwrap();
    let Some(obj) = guard.get_mut(&handle) else {
        return -1;
    };
    match filter {
        0 => negate(&mut obj.img),
        1 => grayscale(&mut obj.img),
        2 => brightness(&mut obj.img, a1),
        3 => contrast(&mut obj.img, a1),
        4 => colorize(&mut obj.img, a1, a2, a3, a4),
        5 | 6 | 7 | 8 | 9 | 10 => {
            let Some((kernel, div)) = builtin_kernel(filter, a1 as f64) else {
                return -1;
            };
            let orig = obj.img.clone();
            convolve3x3(&mut obj.img, &orig, kernel, div, 0.0);
        }
        11 => {
            if !pixelate(&mut obj.img, a1, a2 != 0) {
                return -1;
            }
        }
        12 => {
            if !scatter(&mut obj.img, a1, a2) {
                return -1;
            }
        }
        _ => return -1,
    }
    0
}

/// Applies the 3×3 kernel pushed via [`fbuf`] (`imageconvolution`), dividing by
/// `div` and adding `offset` (both 16.16 fixed-point). Returns `0` on success and
/// `-1` for an unknown handle or a wrong-length kernel.
#[no_mangle]
pub extern "C" fn elephc_img_convolution(handle: i64, div_fixed: i64, offset_fixed: i64) -> i64 {
    let m = fbuf::values();
    if m.len() != 9 {
        return -1;
    }
    let kernel = [m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]];
    let div = div_fixed as f64 / 65536.0;
    let offset = offset_fixed as f64 / 65536.0;
    let mut guard = images().lock().unwrap();
    let Some(obj) = guard.get_mut(&handle) else {
        return -1;
    };
    let orig = obj.img.clone();
    convolve3x3(&mut obj.img, &orig, kernel, div, offset);
    0
}

/// Applies gamma correction (`imagegammacorrect`): each RGB channel becomes
/// `(c/255)^(input/output) * 255`. `in_fixed`/`out_fixed` are 16.16 fixed-point.
/// Returns `0` on success and `-1` for an unknown handle or a non-positive gamma.
#[no_mangle]
pub extern "C" fn elephc_img_gamma(handle: i64, in_fixed: i64, out_fixed: i64) -> i64 {
    let ig = in_fixed as f64 / 65536.0;
    let og = out_fixed as f64 / 65536.0;
    if ig <= 0.0 || og <= 0.0 {
        return -1;
    }
    let g = ig / og;
    let mut guard = images().lock().unwrap();
    let Some(obj) = guard.get_mut(&handle) else {
        return -1;
    };
    let ch = |c: u8| ((c as f64 / 255.0).powf(g) * 255.0).round().clamp(0.0, 255.0) as u8;
    for p in obj.img.pixels_mut() {
        p.0[0] = ch(p.0[0]);
        p.0[1] = ch(p.0[1]);
        p.0[2] = ch(p.0[2]);
    }
    0
}

/// Stores the interpolation method (`imagesetinterpolation`). Unknown handles
/// ignored.
#[no_mangle]
pub extern "C" fn elephc_img_set_interpolation(handle: i64, method: i64) {
    if let Some(obj) = images().lock().unwrap().get_mut(&handle) {
        obj.interpolation = method;
    }
}

/// Returns the stored interpolation method, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_get_interpolation(handle: i64) -> i64 {
    match images().lock().unwrap().get(&handle) {
        Some(obj) => obj.interpolation,
        None => -1,
    }
}

/// Sets the interlace flag (`imageinterlace`). Unknown handles ignored.
#[no_mangle]
pub extern "C" fn elephc_img_set_interlace(handle: i64, on: i64) {
    if let Some(obj) = images().lock().unwrap().get_mut(&handle) {
        obj.interlace = on != 0;
    }
}

/// Returns the interlace flag as `1`/`0`, or `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_img_get_interlace(handle: i64) -> i64 {
    match images().lock().unwrap().get(&handle) {
        Some(obj) => obj.interlace as i64,
        None => -1,
    }
}
