//! Purpose:
//! GD drawing and fill primitives of the image bridge: lines (plain and dashed),
//! rectangles, polygons, ellipses, arcs, flood fills, and line thickness. All are
//! rasterized in pure Rust (Bresenham lines, parametric ellipse/arc outlines,
//! even-odd scanline polygon/ellipse fills, stack-based flood fill).
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `imageline`/`imagedashedline`, `imagerectangle`/`imagefilledrectangle`,
//!   `imagepolygon`/`imageopenpolygon`/`imagefilledpolygon`, `imageellipse`/
//!   `imagefilledellipse`, `imagearc`/`imagefilledarc`, `imagefill`/
//!   `imagefilltoborder`, and `imagesetthickness`.
//!
//! Key details:
//! - Outline primitives honor the image's alpha-blending mode (compositing via
//!   `blend_over`) and line thickness; flood fills set the color directly, like
//!   GD's `imagefill`.
//! - Polygons are built incrementally through a static point buffer
//!   (`poly_reset`/`poly_add` then `poly_line`/`poly_fill`) so the prelude does
//!   not have to marshal a PHP array across the C ABI.
//! - Ellipse and arc outlines are drawn parametrically (dense angle stepping)
//!   rather than via the integer midpoint algorithm; this is visually equivalent
//!   for the supported sizes and keeps the code compact.

use std::f64::consts::PI;
use std::sync::{Mutex, OnceLock};

use image::{Rgba, RgbaImage};

use crate::{ffi_guard, blend_over, images, unpack_color, unpack_pair};

/// Static point buffer for the polygon builder, filled by `elephc_img_poly_add`
/// and consumed by `elephc_img_poly_line` / `elephc_img_poly_fill`.
fn poly_cell() -> &'static Mutex<Vec<(i64, i64)>> {
    static POLY: OnceLock<Mutex<Vec<(i64, i64)>>> = OnceLock::new();
    POLY.get_or_init(Mutex::default)
}

/// Plots a single pixel with bounds checking, compositing over the existing
/// pixel when `blending` is on. Shared with the text renderer.
pub(crate) fn plot(img: &mut RgbaImage, blending: bool, x: i64, y: i64, src: Rgba<u8>) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
        return;
    }
    let (x, y) = (x as u32, y as u32);
    let pixel = if blending {
        blend_over(src, *img.get_pixel(x, y))
    } else {
        src
    };
    img.put_pixel(x, y, pixel);
}

/// Plots a pixel as a `thickness`×`thickness` square brush centered on the point
/// (a 1×1 dot for thickness 1), used by the outline primitives.
fn plot_thick(img: &mut RgbaImage, blending: bool, x: i64, y: i64, src: Rgba<u8>, thickness: i64) {
    if thickness <= 1 {
        plot(img, blending, x, y, src);
        return;
    }
    let lo = -((thickness - 1) / 2);
    let hi = thickness / 2;
    for dy in lo..=hi {
        for dx in lo..=hi {
            plot(img, blending, x + dx, y + dy, src);
        }
    }
}

/// Draws a line with Bresenham's algorithm, stamping each point with the current
/// thickness. With `dash` set, alternates 6-on / 6-off pixels (`imagedashedline`).
fn draw_line(
    img: &mut RgbaImage,
    blending: bool,
    mut x0: i64,
    mut y0: i64,
    x1: i64,
    y1: i64,
    src: Rgba<u8>,
    thickness: i64,
    dash: bool,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut step = 0i64;
    loop {
        if !dash || (step % 12) < 6 {
            plot_thick(img, blending, x0, y0, src, thickness);
        }
        step += 1;
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Fills an axis-aligned rectangle (inclusive of both corners) by row.
fn fill_rect(img: &mut RgbaImage, blending: bool, x1: i64, y1: i64, x2: i64, y2: i64, src: Rgba<u8>) {
    let (lx, hx) = (x1.min(x2), x1.max(x2));
    let (ly, hy) = (y1.min(y2), y1.max(y2));
    for y in ly..=hy {
        for x in lx..=hx {
            plot(img, blending, x, y, src);
        }
    }
}

/// Draws an ellipse outline centered at `(cx, cy)` with full width/height `w`/`h`
/// by stepping the parametric angle densely.
fn ellipse_outline(
    img: &mut RgbaImage,
    blending: bool,
    cx: i64,
    cy: i64,
    w: i64,
    h: i64,
    src: Rgba<u8>,
    thickness: i64,
) {
    let rx = w / 2;
    let ry = h / 2;
    if rx <= 0 || ry <= 0 {
        return;
    }
    let steps = (4 * (rx + ry)).max(16);
    for i in 0..steps {
        let theta = 2.0 * PI * (i as f64) / (steps as f64);
        let x = cx + (rx as f64 * theta.cos()).round() as i64;
        let y = cy + (ry as f64 * theta.sin()).round() as i64;
        plot_thick(img, blending, x, y, src, thickness);
    }
}

/// Fills an ellipse centered at `(cx, cy)` with full width/height `w`/`h` by
/// computing the horizontal span for each row.
fn fill_ellipse(img: &mut RgbaImage, blending: bool, cx: i64, cy: i64, w: i64, h: i64, src: Rgba<u8>) {
    let rx = w / 2;
    let ry = h / 2;
    if rx <= 0 || ry <= 0 {
        return;
    }
    for dy in -ry..=ry {
        let frac = 1.0 - (dy as f64 / ry as f64).powi(2);
        if frac < 0.0 {
            continue;
        }
        let dx = (rx as f64 * frac.sqrt()).round() as i64;
        for x in (cx - dx)..=(cx + dx) {
            plot(img, blending, x, cy + dy, src);
        }
    }
}

/// Returns true if `ang` (degrees) lies within the clockwise sweep from `start`
/// to `end`, handling wraparound and full sweeps.
fn angle_in_range(ang: f64, start: f64, end: f64) -> bool {
    if (end - start).abs() >= 360.0 {
        return true;
    }
    let s = start.rem_euclid(360.0);
    let e = end.rem_euclid(360.0);
    let a = ang.rem_euclid(360.0);
    if s <= e {
        a >= s && a <= e
    } else {
        a >= s || a <= e
    }
}

/// Draws an arc outline from `start` to `end` degrees (GD convention: 0° at 3
/// o'clock, increasing clockwise) by parametric stepping.
fn arc_outline(
    img: &mut RgbaImage,
    blending: bool,
    cx: i64,
    cy: i64,
    w: i64,
    h: i64,
    start: f64,
    end: f64,
    src: Rgba<u8>,
    thickness: i64,
) {
    let rx = w / 2;
    let ry = h / 2;
    if rx <= 0 || ry <= 0 {
        return;
    }
    let sweep = if end >= start { end - start } else { end + 360.0 - start };
    let steps = ((sweep / 360.0) * 4.0 * (rx + ry) as f64).max(2.0) as i64;
    for i in 0..=steps {
        let deg = start + sweep * (i as f64) / (steps as f64);
        let theta = deg * PI / 180.0;
        let x = cx + (rx as f64 * theta.cos()).round() as i64;
        let y = cy + (ry as f64 * theta.sin()).round() as i64;
        plot_thick(img, blending, x, y, src, thickness);
    }
}

/// Fills a pie slice of an ellipse: every in-ellipse pixel whose (aspect-
/// normalized) angle lies in the `start`..`end` sweep.
fn fill_pie(
    img: &mut RgbaImage,
    blending: bool,
    cx: i64,
    cy: i64,
    w: i64,
    h: i64,
    start: f64,
    end: f64,
    src: Rgba<u8>,
) {
    let rx = w / 2;
    let ry = h / 2;
    if rx <= 0 || ry <= 0 {
        return;
    }
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let fx = dx as f64 / rx as f64;
            let fy = dy as f64 / ry as f64;
            if fx * fx + fy * fy > 1.0 {
                continue;
            }
            let ang = fy.atan2(fx) * 180.0 / PI;
            if angle_in_range(ang, start, end) {
                plot(img, blending, cx + dx, cy + dy, src);
            }
        }
    }
}

/// Flood-fills the region of pixels matching the seed color at `(x, y)`,
/// replacing them with `src` (set directly, like GD's `imagefill`).
fn flood_fill(img: &mut RgbaImage, x: i64, y: i64, src: Rgba<u8>) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
        return;
    }
    let target = *img.get_pixel(x as u32, y as u32);
    if target == src {
        return;
    }
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx as u32 >= img.width() || cy as u32 >= img.height() {
            continue;
        }
        if *img.get_pixel(cx as u32, cy as u32) != target {
            continue;
        }
        img.put_pixel(cx as u32, cy as u32, src);
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
}

/// Flood-fills outward from `(x, y)`, stopping at the `border` color, filling
/// every reachable non-border pixel with `src` (`imagefilltoborder`).
fn fill_to_border(img: &mut RgbaImage, x: i64, y: i64, border: Rgba<u8>, src: Rgba<u8>) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
        return;
    }
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx as u32 >= img.width() || cy as u32 >= img.height() {
            continue;
        }
        let p = *img.get_pixel(cx as u32, cy as u32);
        if p == border || p == src {
            continue;
        }
        img.put_pixel(cx as u32, cy as u32, src);
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
}

/// Even-odd scanline fill of the closed polygon described by `pts`.
fn fill_polygon(img: &mut RgbaImage, blending: bool, pts: &[(i64, i64)], src: Rgba<u8>) {
    if pts.len() < 3 {
        return;
    }
    let ymin = pts.iter().map(|p| p.1).min().unwrap();
    let ymax = pts.iter().map(|p| p.1).max().unwrap();
    let n = pts.len();
    for y in ymin..=ymax {
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..n {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % n];
            // Half-open edge test so shared vertices are counted once.
            if (y1 <= y && y2 > y) || (y2 <= y && y1 > y) {
                let t = (y - y1) as f64 / (y2 - y1) as f64;
                xs.push(x1 as f64 + t * (x2 - x1) as f64);
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        let mut i = 0;
        while i + 1 < xs.len() {
            let xa = xs[i].ceil() as i64;
            let xb = xs[i + 1].floor() as i64;
            for x in xa..=xb {
                plot(img, blending, x, y, src);
            }
            i += 2;
        }
    }
}

/// Sets the current line thickness. Unknown handles are ignored.
#[no_mangle]
pub extern "C" fn elephc_img_set_thickness(handle: i64, thickness: i64) {
    ffi_guard((), move || {
        if let Some(obj) = images().lock().unwrap().get_mut(&handle) {
            obj.thickness = thickness.max(1);
        }
    })
}

/// Draws a straight line between two points with the current thickness.
#[no_mangle]
pub extern "C" fn elephc_img_line(handle: i64, x1: i64, y1: i64, x2: i64, y2: i64, color: i64) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let (b, t) = (obj.alpha_blending, obj.thickness);
            draw_line(&mut obj.img, b, x1, y1, x2, y2, unpack_color(color), t, false);
        }
    })
}

/// Draws a dashed line between two points.
#[no_mangle]
pub extern "C" fn elephc_img_dashed_line(handle: i64, x1: i64, y1: i64, x2: i64, y2: i64, color: i64) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let (b, t) = (obj.alpha_blending, obj.thickness);
            draw_line(&mut obj.img, b, x1, y1, x2, y2, unpack_color(color), t, true);
        }
    })
}

/// Draws a rectangle outline through its two opposite corners.
#[no_mangle]
pub extern "C" fn elephc_img_rectangle(handle: i64, x1: i64, y1: i64, x2: i64, y2: i64, color: i64) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let (b, t) = (obj.alpha_blending, obj.thickness);
            let src = unpack_color(color);
            draw_line(&mut obj.img, b, x1, y1, x2, y1, src, t, false);
            draw_line(&mut obj.img, b, x2, y1, x2, y2, src, t, false);
            draw_line(&mut obj.img, b, x2, y2, x1, y2, src, t, false);
            draw_line(&mut obj.img, b, x1, y2, x1, y1, src, t, false);
        }
    })
}

/// Draws a filled rectangle through its two opposite corners.
#[no_mangle]
pub extern "C" fn elephc_img_filled_rectangle(
    handle: i64,
    x1: i64,
    y1: i64,
    x2: i64,
    y2: i64,
    color: i64,
) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let b = obj.alpha_blending;
            fill_rect(&mut obj.img, b, x1, y1, x2, y2, unpack_color(color));
        }
    })
}

/// Draws an ellipse outline centered at `(cx, cy)` with the given width/height.
#[no_mangle]
pub extern "C" fn elephc_img_ellipse(handle: i64, cx: i64, cy: i64, w: i64, h: i64, color: i64) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let (b, t) = (obj.alpha_blending, obj.thickness);
            ellipse_outline(&mut obj.img, b, cx, cy, w, h, unpack_color(color), t);
        }
    })
}

/// Draws a filled ellipse centered at `(cx, cy)` with the given width/height.
#[no_mangle]
pub extern "C" fn elephc_img_filled_ellipse(
    handle: i64,
    cx: i64,
    cy: i64,
    w: i64,
    h: i64,
    color: i64,
) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let b = obj.alpha_blending;
            fill_ellipse(&mut obj.img, b, cx, cy, w, h, unpack_color(color));
        }
    })
}

/// Draws an arc outline (degrees, GD's clockwise-from-3-o'clock convention).
/// `cxy` packs `(cx, cy)` and `wh` packs `(w, h)` — see [`unpack_pair`].
#[no_mangle]
pub extern "C" fn elephc_img_arc(handle: i64, cxy: i64, wh: i64, start: i64, end: i64, color: i64) {
    ffi_guard((), move || {
        let (cx, cy) = unpack_pair(cxy);
        let (w, h) = unpack_pair(wh);
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let (b, t) = (obj.alpha_blending, obj.thickness);
            arc_outline(&mut obj.img, b, cx, cy, w, h, start as f64, end as f64, unpack_color(color), t);
        }
    })
}

/// Fills the pie slice of an arc from `start` to `end` degrees. The GD `$style`
/// bitmask (pie vs. chord vs. no-fill) is resolved in the prelude, which routes
/// the no-fill case to `elephc_img_arc`; this entry point always fills a pie
/// (kept at eight integer arguments to fit the ABI register limit).
#[no_mangle]
pub extern "C" fn elephc_img_filled_arc(
    handle: i64,
    cxy: i64,
    wh: i64,
    start: i64,
    end: i64,
    color: i64,
) {
    ffi_guard((), move || {
        let (cx, cy) = unpack_pair(cxy);
        let (w, h) = unpack_pair(wh);
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let b = obj.alpha_blending;
            fill_pie(&mut obj.img, b, cx, cy, w, h, start as f64, end as f64, unpack_color(color));
        }
    })
}

/// Flood-fills from `(x, y)` with the given color (`imagefill`).
#[no_mangle]
pub extern "C" fn elephc_img_fill(handle: i64, x: i64, y: i64, color: i64) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            flood_fill(&mut obj.img, x, y, unpack_color(color));
        }
    })
}

/// Flood-fills from `(x, y)` up to the `border` color (`imagefilltoborder`).
#[no_mangle]
pub extern "C" fn elephc_img_fill_to_border(handle: i64, x: i64, y: i64, border: i64, color: i64) {
    ffi_guard((), move || {
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            fill_to_border(&mut obj.img, x, y, unpack_color(border), unpack_color(color));
        }
    })
}

/// Clears the polygon point buffer before a new polygon is described.
#[no_mangle]
pub extern "C" fn elephc_img_poly_reset() {
    ffi_guard((), move || {
        poly_cell().lock().unwrap().clear();
    })
}

/// Appends a point to the polygon buffer.
#[no_mangle]
pub extern "C" fn elephc_img_poly_add(x: i64, y: i64) {
    ffi_guard((), move || {
        poly_cell().lock().unwrap().push((x, y));
    })
}

/// Draws the buffered polygon's outline; `closed` (1) connects the last point
/// back to the first (`imagepolygon`), `0` leaves it open (`imageopenpolygon`).
#[no_mangle]
pub extern "C" fn elephc_img_poly_line(handle: i64, color: i64, closed: i64) {
    ffi_guard((), move || {
        let pts = poly_cell().lock().unwrap().clone();
        if pts.len() < 2 {
            return;
        }
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let (b, t) = (obj.alpha_blending, obj.thickness);
            let src = unpack_color(color);
            for i in 0..pts.len() - 1 {
                draw_line(&mut obj.img, b, pts[i].0, pts[i].1, pts[i + 1].0, pts[i + 1].1, src, t, false);
            }
            if closed != 0 {
                let last = pts.len() - 1;
                draw_line(&mut obj.img, b, pts[last].0, pts[last].1, pts[0].0, pts[0].1, src, t, false);
            }
        }
    })
}

/// Fills the buffered polygon (`imagefilledpolygon`).
#[no_mangle]
pub extern "C" fn elephc_img_poly_fill(handle: i64, color: i64) {
    ffi_guard((), move || {
        let pts = poly_cell().lock().unwrap().clone();
        let mut guard = images().lock().unwrap();
        if let Some(obj) = guard.get_mut(&handle) {
            let b = obj.alpha_blending;
            fill_polygon(&mut obj.img, b, &pts, unpack_color(color));
        }
    })
}
