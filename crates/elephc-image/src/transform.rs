//! Purpose:
//! GD geometric transforms of the image bridge: region copies (plain, merge,
//! merge-gray, resized, resampled), whole-image scaling, cropping (explicit and
//! auto), flipping, arbitrary-angle rotation, and affine transforms. Copies act in
//! place on the destination; scale/crop/rotate/affine allocate and return a new
//! image handle.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `imagecopy`/`imagecopymerge`/`imagecopymergegray`/`imagecopyresized`/
//!   `imagecopyresampled`, `imagescale`, `imagecrop`/`imagecropauto`, `imageflip`,
//!   `imagerotate`, and `imageaffine`.
//!
//! Key details:
//! - Coordinate/size pairs are packed two-per-`i64` (see [`crate::unpack_pair`]) so
//!   every entry point stays within the six-integer-argument x86_64 extern ABI
//!   limit; the rotate angle and affine matrix cross as fixed-point integers (the
//!   angle in millidegrees, the matrix via [`crate::fbuf`]).
//! - Copies composite over the destination when its alpha-blending flag is on and
//!   overwrite otherwise, matching `imagesetpixel`. Self-copy (`src == dst`) is
//!   safe because the source region is cloned before the destination is mutated.
//! - Resampling uses a triangle (bilinear) filter and resizing uses
//!   nearest-neighbour, matching GD's `imagecopyresampled` vs. `imagecopyresized`.
//!   Rotation and affine sample nearest-neighbour and fill exposed area with the
//!   background / transparent, keeping solid colors exact.

use image::{imageops, imageops::FilterType, Rgba, RgbaImage};

use crate::{ffi_guard, blend_over, fbuf, images, insert_image, unpack_color, unpack_pair, ImageObj};

/// GD interpolation code for nearest-neighbour sampling (`IMG_NEAREST_NEIGHBOUR`);
/// any other stored interpolation selects a linear filter for scaling.
const IMG_NEAREST_NEIGHBOUR: i64 = 16;

/// Writes a source pixel into the destination at `(x, y)`, compositing over the
/// existing pixel when `blending` is on and overwriting otherwise. Out-of-bounds
/// coordinates are ignored.
fn put_blend(dst: &mut RgbaImage, blending: bool, x: i64, y: i64, src: Rgba<u8>) {
    if x < 0 || y < 0 || x as u32 >= dst.width() || y as u32 >= dst.height() {
        return;
    }
    let (x, y) = (x as u32, y as u32);
    let pixel = if blending {
        blend_over(src, *dst.get_pixel(x, y))
    } else {
        src
    };
    dst.put_pixel(x, y, pixel);
}

/// Clones the `sw`×`sh` source region anchored at `(sx, sy)` into a fresh buffer,
/// reading out-of-bounds source pixels as transparent. Cloning first lets a copy
/// target the same image it reads from without aliasing the handle table.
fn clone_region(src: &RgbaImage, sx: i64, sy: i64, sw: i64, sh: i64) -> RgbaImage {
    let (sw, sh) = (sw.max(0) as u32, sh.max(0) as u32);
    let mut region = RgbaImage::from_pixel(sw.max(1), sh.max(1), Rgba([0, 0, 0, 0]));
    for j in 0..sh {
        for i in 0..sw {
            let (xx, yy) = (sx + i as i64, sy + j as i64);
            if xx >= 0 && yy >= 0 && (xx as u32) < src.width() && (yy as u32) < src.height() {
                region.put_pixel(i, j, *src.get_pixel(xx as u32, yy as u32));
            }
        }
    }
    region
}

/// Copies a source region into the destination at `(dx, dy)`. Backs `imagecopy`.
/// `dxy`/`sxy`/`swh` pack the destination origin, source origin, and source size.
#[no_mangle]
pub extern "C" fn elephc_img_copy(dst_h: i64, src_h: i64, dxy: i64, sxy: i64, swh: i64) -> i64 {
    ffi_guard(-1, move || {
        let (dx, dy) = unpack_pair(dxy);
        let (sx, sy) = unpack_pair(sxy);
        let (sw, sh) = unpack_pair(swh);
        let mut guard = images().lock().unwrap();
        let Some(src) = guard.get(&src_h) else {
            return -1;
        };
        let region = clone_region(&src.img, sx, sy, sw, sh);
        let Some(dst) = guard.get_mut(&dst_h) else {
            return -1;
        };
        let blending = dst.alpha_blending;
        for j in 0..region.height() {
            for i in 0..region.width() {
                put_blend(&mut dst.img, blending, dx + i as i64, dy + j as i64, *region.get_pixel(i, j));
            }
        }
        0
    })
}

/// Merges a source region into the destination at `(dx, dy)` with `pct` opacity
/// (0-100). Backs `imagecopymerge`: each channel is a linear blend
/// `dst*(1-pct) + src*pct`, ignoring alpha (treated opaque), like GD.
#[no_mangle]
pub extern "C" fn elephc_img_copy_merge(
    dst_h: i64,
    src_h: i64,
    dxy: i64,
    sxy: i64,
    swh: i64,
    pct: i64,
) -> i64 {
    ffi_guard(-1, move || {
        copy_merge_impl(dst_h, src_h, dxy, sxy, swh, pct, false)
    })
}

/// Merges a source region into the destination with `pct` opacity after first
/// converting the destination pixels to grayscale, preserving the source hue.
/// Backs `imagecopymergegray` (an approximation of GD's HSV-based variant).
#[no_mangle]
pub extern "C" fn elephc_img_copy_merge_gray(
    dst_h: i64,
    src_h: i64,
    dxy: i64,
    sxy: i64,
    swh: i64,
    pct: i64,
) -> i64 {
    ffi_guard(-1, move || {
        copy_merge_impl(dst_h, src_h, dxy, sxy, swh, pct, true)
    })
}

/// Shared body for `imagecopymerge`/`imagecopymergegray`: with `gray` set the
/// destination pixel is desaturated before the linear `pct` blend.
fn copy_merge_impl(
    dst_h: i64,
    src_h: i64,
    dxy: i64,
    sxy: i64,
    swh: i64,
    pct: i64,
    gray: bool,
) -> i64 {
    let (dx, dy) = unpack_pair(dxy);
    let (sx, sy) = unpack_pair(sxy);
    let (sw, sh) = unpack_pair(swh);
    let p = (pct.clamp(0, 100) as f64) / 100.0;
    let mut guard = images().lock().unwrap();
    let Some(src) = guard.get(&src_h) else {
        return -1;
    };
    let region = clone_region(&src.img, sx, sy, sw, sh);
    let Some(dst) = guard.get_mut(&dst_h) else {
        return -1;
    };
    for j in 0..region.height() {
        for i in 0..region.width() {
            let (tx, ty) = (dx + i as i64, dy + j as i64);
            if tx < 0 || ty < 0 || tx as u32 >= dst.img.width() || ty as u32 >= dst.img.height() {
                continue;
            }
            let s = region.get_pixel(i, j).0;
            let mut d = dst.img.get_pixel(tx as u32, ty as u32).0;
            if gray {
                let lum = (0.299 * d[0] as f64 + 0.587 * d[1] as f64 + 0.114 * d[2] as f64) as f64;
                d[0] = lum.round() as u8;
                d[1] = d[0];
                d[2] = d[0];
            }
            let mix = |sv: u8, dv: u8| -> u8 {
                (dv as f64 * (1.0 - p) + sv as f64 * p).round().clamp(0.0, 255.0) as u8
            };
            dst.img.put_pixel(
                tx as u32,
                ty as u32,
                Rgba([mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]), d[3]]),
            );
        }
    }
    0
}

/// Copies and scales a source region into a destination region. Backs
/// `imagecopyresized` (`resample` = false → nearest-neighbour) and
/// `imagecopyresampled` (`resample` = true → bilinear). `dwh`/`swh` pack the
/// destination and source sizes.
fn copy_scaled(
    dst_h: i64,
    src_h: i64,
    dxy: i64,
    sxy: i64,
    dwh: i64,
    swh: i64,
    resample: bool,
) -> i64 {
    let (dx, dy) = unpack_pair(dxy);
    let (sx, sy) = unpack_pair(sxy);
    let (dw, dh) = unpack_pair(dwh);
    let (sw, sh) = unpack_pair(swh);
    if dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0 {
        return -1;
    }
    let mut guard = images().lock().unwrap();
    let Some(src) = guard.get(&src_h) else {
        return -1;
    };
    let region = clone_region(&src.img, sx, sy, sw, sh);
    let filter = if resample { FilterType::Triangle } else { FilterType::Nearest };
    let scaled = imageops::resize(&region, dw as u32, dh as u32, filter);
    let Some(dst) = guard.get_mut(&dst_h) else {
        return -1;
    };
    let blending = dst.alpha_blending;
    for j in 0..scaled.height() {
        for i in 0..scaled.width() {
            put_blend(&mut dst.img, blending, dx + i as i64, dy + j as i64, *scaled.get_pixel(i, j));
        }
    }
    0
}

/// Backs `imagecopyresized` (nearest-neighbour region copy with scaling).
#[no_mangle]
pub extern "C" fn elephc_img_copy_resized(
    dst_h: i64,
    src_h: i64,
    dxy: i64,
    sxy: i64,
    dwh: i64,
    swh: i64,
) -> i64 {
    ffi_guard(-1, move || {
        copy_scaled(dst_h, src_h, dxy, sxy, dwh, swh, false)
    })
}

/// Backs `imagecopyresampled` (bilinear region copy with scaling).
#[no_mangle]
pub extern "C" fn elephc_img_copy_resampled(
    dst_h: i64,
    src_h: i64,
    dxy: i64,
    sxy: i64,
    dwh: i64,
    swh: i64,
) -> i64 {
    ffi_guard(-1, move || {
        copy_scaled(dst_h, src_h, dxy, sxy, dwh, swh, true)
    })
}

/// Scales an image to `new_w`×`new_h`, returning a new handle (`imagescale`). A
/// negative `new_h` preserves the aspect ratio. `mode` = `IMG_NEAREST_NEIGHBOUR`
/// selects nearest sampling; anything else uses a bilinear (triangle) filter.
#[no_mangle]
pub extern "C" fn elephc_img_scale(src_h: i64, new_w: i64, new_h: i64, mode: i64) -> i64 {
    ffi_guard(-1, move || {
        if new_w <= 0 {
            return -1;
        }
        let guard = images().lock().unwrap();
        let Some(src) = guard.get(&src_h) else {
            return -1;
        };
        let (w0, h0) = (src.img.width(), src.img.height());
        let nh = if new_h < 0 {
            ((new_w as f64) * (h0 as f64) / (w0 as f64)).round().max(1.0) as u32
        } else if new_h == 0 {
            return -1;
        } else {
            new_h as u32
        };
        let filter = if mode == IMG_NEAREST_NEIGHBOUR { FilterType::Nearest } else { FilterType::Triangle };
        let scaled = imageops::resize(&src.img, new_w as u32, nh, filter);
        let truecolor = src.truecolor;
        drop(guard);
        insert_image(ImageObj::new(scaled, truecolor))
    })
}

/// Crops a `w`×`h` rectangle anchored at `(x, y)`, returning a new handle
/// (`imagecrop`). Out-of-bounds areas are filled transparent, matching GD's
/// behavior for a rectangle that extends past the source.
#[no_mangle]
pub extern "C" fn elephc_img_crop(src_h: i64, x: i64, y: i64, w: i64, h: i64) -> i64 {
    ffi_guard(-1, move || {
        if w <= 0 || h <= 0 {
            return -1;
        }
        let guard = images().lock().unwrap();
        let Some(src) = guard.get(&src_h) else {
            return -1;
        };
        let cropped = clone_region(&src.img, x, y, w, h);
        let truecolor = src.truecolor;
        drop(guard);
        insert_image(ImageObj::new(cropped, truecolor))
    })
}

/// Returns whether a pixel counts as "border" for `imagecropauto`, by mode:
/// transparent (mode 1), black (2), white (3), within `thr` of `refc` (5,
/// threshold), or an exact match of the reference color (default / sides).
fn is_border(p: Rgba<u8>, mode: i64, refc: Rgba<u8>, thr: f64) -> bool {
    match mode {
        1 => p.0[3] == 0,
        5 => {
            let d = ((p.0[0] as f64 - refc.0[0] as f64).powi(2)
                + (p.0[1] as f64 - refc.0[1] as f64).powi(2)
                + (p.0[2] as f64 - refc.0[2] as f64).powi(2))
            .sqrt();
            d <= thr
        }
        _ => p == refc,
    }
}

/// Auto-crops a uniform border, returning a new handle (`imagecropauto`). `mode`
/// selects the border color (default/sides = top-left pixel, transparent, black,
/// white, or threshold around `color`); `threshold_permille` is the mode-5
/// tolerance as parts-per-thousand of the RGB diagonal. Returns `-1` when the
/// whole image is border (nothing to keep) or dimensions are degenerate.
#[no_mangle]
pub extern "C" fn elephc_img_crop_auto(
    src_h: i64,
    mode: i64,
    color: i64,
    threshold_permille: i64,
) -> i64 {
    ffi_guard(-1, move || {
        let guard = images().lock().unwrap();
        let Some(src) = guard.get(&src_h) else {
            return -1;
        };
        let img = &src.img;
        let (w, h) = (img.width(), img.height());
        if w == 0 || h == 0 {
            return -1;
        }
        let refc = match mode {
            2 => Rgba([0, 0, 0, 255]),
            3 => Rgba([255, 255, 255, 255]),
            5 => unpack_color(color),
            _ => *img.get_pixel(0, 0),
        };
        // Max RGB Euclidean distance is sqrt(3)*255; scale the per-mille threshold to it.
        let thr = (threshold_permille as f64 / 1000.0) * (3.0_f64.sqrt() * 255.0);
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
        let mut found = false;
        for y in 0..h {
            for x in 0..w {
                if !is_border(*img.get_pixel(x, y), mode, refc, thr) {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if !found {
            return -1;
        }
        let cropped = clone_region(
            img,
            min_x as i64,
            min_y as i64,
            (max_x - min_x + 1) as i64,
            (max_y - min_y + 1) as i64,
        );
        let truecolor = src.truecolor;
        drop(guard);
        insert_image(ImageObj::new(cropped, truecolor))
    })
}

/// Flips an image in place (`imageflip`): mode 1 horizontal, 2 vertical, 3 both.
#[no_mangle]
pub extern "C" fn elephc_img_flip(handle: i64, mode: i64) -> i64 {
    ffi_guard(-1, move || {
        let mut guard = images().lock().unwrap();
        let Some(obj) = guard.get_mut(&handle) else {
            return -1;
        };
        match mode {
            1 => imageops::flip_horizontal_in_place(&mut obj.img),
            2 => imageops::flip_vertical_in_place(&mut obj.img),
            3 => {
                imageops::flip_horizontal_in_place(&mut obj.img);
                imageops::flip_vertical_in_place(&mut obj.img);
            }
            _ => return -1,
        }
        0
    })
}

/// Rotates an image counter-clockwise by `angle_mdeg` millidegrees about its
/// center, returning a new handle (`imagerotate`). Right-angle multiples use exact
/// pixel permutation; other angles inverse-map with nearest-neighbour sampling and
/// fill exposed area with `bgcolor`. The result is enlarged to fit the rotation.
#[no_mangle]
pub extern "C" fn elephc_img_rotate(src_h: i64, angle_mdeg: i64, bgcolor: i64) -> i64 {
    ffi_guard(-1, move || {
        let guard = images().lock().unwrap();
        let Some(src) = guard.get(&src_h) else {
            return -1;
        };
        let truecolor = src.truecolor;
        let deg = (angle_mdeg as f64 / 1000.0).rem_euclid(360.0);
        // Right-angle multiples are exact permutations (PHP rotates counter-clockwise,
        // so a CCW quarter turn is image's clockwise rotate270, and vice versa).
        let out = if (deg - 0.0).abs() < 1e-6 {
            src.img.clone()
        } else if (deg - 90.0).abs() < 1e-6 {
            imageops::rotate270(&src.img)
        } else if (deg - 180.0).abs() < 1e-6 {
            imageops::rotate180(&src.img)
        } else if (deg - 270.0).abs() < 1e-6 {
            imageops::rotate90(&src.img)
        } else {
            rotate_arbitrary(&src.img, deg, unpack_color(bgcolor))
        };
        drop(guard);
        insert_image(ImageObj::new(out, truecolor))
    })
}

/// Rotates `img` counter-clockwise by `deg` degrees about its center into a buffer
/// sized to the rotated bounding box, sampling nearest-neighbour and filling
/// uncovered pixels with `bg`.
fn rotate_arbitrary(img: &RgbaImage, deg: f64, bg: Rgba<u8>) -> RgbaImage {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let rad = deg.to_radians();
    let (c, s) = (rad.cos(), rad.sin());
    let new_w = (w * c.abs() + h * s.abs()).ceil().max(1.0);
    let new_h = (w * s.abs() + h * c.abs()).ceil().max(1.0);
    let mut out = RgbaImage::from_pixel(new_w as u32, new_h as u32, bg);
    let (cx0, cy0) = (w / 2.0, h / 2.0);
    let (cx1, cy1) = (new_w / 2.0, new_h / 2.0);
    for dy in 0..out.height() {
        for dx in 0..out.width() {
            let rx = dx as f64 + 0.5 - cx1;
            let ry = dy as f64 + 0.5 - cy1;
            // Inverse of a CCW rotation in screen space (y down) maps dst → src.
            let sxf = c * rx + s * ry + cx0;
            let syf = -s * rx + c * ry + cy0;
            let (sxi, syi) = (sxf.floor() as i64, syf.floor() as i64);
            if sxi >= 0 && syi >= 0 && (sxi as u32) < img.width() && (syi as u32) < img.height() {
                out.put_pixel(dx, dy, *img.get_pixel(sxi as u32, syi as u32));
            }
        }
    }
    out
}

/// Applies the affine matrix pushed via [`fbuf`] to an image, returning a new
/// handle (`imageaffine`). The 6-element matrix `[a, b, c, d, e, f]` maps a source
/// point `(x, y)` to `(a*x + c*y + e, b*x + d*y + f)`; the output is sized to the
/// transformed bounding box and sampled nearest-neighbour, exposed area
/// transparent. Returns `-1` for an unknown handle, a wrong-length matrix, or a
/// singular (non-invertible) transform.
#[no_mangle]
pub extern "C" fn elephc_img_affine(src_h: i64) -> i64 {
    ffi_guard(-1, move || {
        let m = fbuf::values();
        if m.len() != 6 {
            return -1;
        }
        let (a, b, c, d, e, f) = (m[0], m[1], m[2], m[3], m[4], m[5]);
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return -1;
        }
        let guard = images().lock().unwrap();
        let Some(src) = guard.get(&src_h) else {
            return -1;
        };
        let img = &src.img;
        let (w, h) = (img.width() as f64, img.height() as f64);
        // Transform the four corners to find the output bounding box.
        let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for (x, y) in corners {
            let tx = a * x + c * y + e;
            let ty = b * x + d * y + f;
            min_x = min_x.min(tx);
            min_y = min_y.min(ty);
            max_x = max_x.max(tx);
            max_y = max_y.max(ty);
        }
        let new_w = (max_x - min_x).ceil().max(1.0);
        let new_h = (max_y - min_y).ceil().max(1.0);
        // Inverse 2×2 for mapping output pixels back to source coordinates.
        let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
        let mut out = RgbaImage::from_pixel(new_w as u32, new_h as u32, Rgba([0, 0, 0, 0]));
        for dy in 0..out.height() {
            for dx in 0..out.width() {
                let wx = dx as f64 + 0.5 + min_x;
                let wy = dy as f64 + 0.5 + min_y;
                let sxf = ia * (wx - e) + ic * (wy - f);
                let syf = ib * (wx - e) + id * (wy - f);
                let (sxi, syi) = (sxf.floor() as i64, syf.floor() as i64);
                if sxi >= 0 && syi >= 0 && (sxi as u32) < img.width() && (syi as u32) < img.height() {
                    out.put_pixel(dx, dy, *img.get_pixel(sxi as u32, syi as u32));
                }
            }
        }
        let truecolor = src.truecolor;
        drop(guard);
        insert_image(ImageObj::new(out, truecolor))
    })
}
