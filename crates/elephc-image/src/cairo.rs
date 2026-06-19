//! Purpose:
//! Pure-Rust Cairo bridge: backs the PHP `Cairo*` object API on `tiny-skia`. Owns
//! an image-surface table (each a `tiny_skia::Pixmap`), a drawing-context table
//! (path builder, source paint, line state, transformation matrix, save/restore
//! stack), and a pattern table (solid / linear / radial gradient specs). Paths are
//! accumulated in device space (each point mapped through the current matrix as it
//! is added), so fill/stroke raster with the identity transform.
//!
//! Called from:
//! - the image prelude's `extern "elephc_image"` block (Cairo classes:
//!   `CairoImageSurface`, `CairoContext`, `CairoSolidPattern`,
//!   `CairoLinearGradient`, `CairoRadialGradient`).
//!
//! Key details:
//! - Geometry crosses the C ABI as fixed-point milli-units (`value * 1000`, i32),
//!   with x/y pairs packed into one i64 (`unpack_pair`) to respect the x86_64
//!   six-integer-argument limit. Colors cross as packed RGBA8 (`r<<24|g<<16|b<<8|a`).
//!   Angles are milli-radians.
//! - Line width is in user space; it is scaled by the matrix's geometric-mean
//!   scale at stroke time. Arcs are sampled into line segments. PDF/PS/SVG
//!   surfaces and FreeType text have no pure-Rust path and are documented gaps
//!   handled (by throwing) in the prelude, not here.

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use image::ImageReader;
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, Path, PathBuilder,
    Pixmap, Point, RadialGradient, Shader, SpreadMode, Stroke, Transform,
};

use crate::codec::set_encoded;
use crate::{cstr_arg, unpack_pair};

/// The paint source currently selected on a context.
enum CairoSource {
    /// A solid RGBA color.
    Solid([u8; 4]),
    /// A prebuilt gradient shader (endpoints already mapped to device space).
    Shader(Shader<'static>),
}

/// One saved drawing state for `cairo_save` / `cairo_restore`.
struct SavedState {
    source: CairoSource,
    line_width: f64,
    line_cap: LineCap,
    line_join: LineJoin,
    fill_rule: FillRule,
    ctm: Transform,
}

/// A drawing context targeting one surface.
struct CairoCtx {
    surface: i64,
    pb: PathBuilder,
    has_current: bool,
    cur: Point,
    sub_start: Point,
    source: CairoSource,
    line_width: f64,
    line_cap: LineCap,
    line_join: LineJoin,
    fill_rule: FillRule,
    ctm: Transform,
    stack: Vec<SavedState>,
}

/// A pattern's geometry and color stops, used to build a shader at `set_source`.
enum CairoPatternKind {
    Solid([u8; 4]),
    Linear { p0: Point, p1: Point },
    Radial { c1: Point, r1: f32 },
}

/// A reusable paint pattern (solid or gradient) with its color stops.
struct CairoPattern {
    kind: CairoPatternKind,
    stops: Vec<(f32, Color)>,
}

/// Returns the global surface (pixmap) handle table.
fn surfaces() -> &'static Mutex<HashMap<i64, Pixmap>> {
    static T: OnceLock<Mutex<HashMap<i64, Pixmap>>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

/// Returns the global drawing-context handle table.
fn contexts() -> &'static Mutex<HashMap<i64, CairoCtx>> {
    static T: OnceLock<Mutex<HashMap<i64, CairoCtx>>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

/// Returns the global pattern handle table.
fn patterns() -> &'static Mutex<HashMap<i64, CairoPattern>> {
    static T: OnceLock<Mutex<HashMap<i64, CairoPattern>>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

/// Allocates the next monotonic Cairo handle id (shared across all three tables).
fn next_id() -> i64 {
    static N: AtomicI64 = AtomicI64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Converts a fixed-point milli-unit integer to an `f32` coordinate.
fn fx(v: i64) -> f32 {
    v as f32 / 1000.0
}

/// Unpacks a packed (x, y) milli-unit pair into an `f32` point (no transform).
fn pt(packed: i64) -> Point {
    let (x, y) = unpack_pair(packed);
    Point::from_xy(fx(x), fx(y))
}

/// Unpacks a packed RGBA8 color integer (`r<<24|g<<16|b<<8|a`) into 4 bytes.
fn rgba(packed: i64) -> [u8; 4] {
    [
        ((packed >> 24) & 0xFF) as u8,
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ]
}

/// Builds a tiny-skia `Color` from packed RGBA8.
fn color(packed: i64) -> Color {
    let c = rgba(packed);
    Color::from_rgba8(c[0], c[1], c[2], c[3])
}

/// Returns the matrix's geometric-mean scale (`sqrt(|det|)`), used to map a
/// user-space line width to device space.
fn matrix_scale(t: &Transform) -> f64 {
    let det = (t.sx * t.sy - t.kx * t.ky).abs() as f64;
    det.sqrt()
}

/// Maps a point through the context's current matrix into device space.
fn to_device(ctx: &CairoCtx, p: Point) -> Point {
    let mut buf = [p];
    ctx.ctm.map_points(&mut buf);
    buf[0]
}

/// Creates an RGBA8 image surface of the given size. Returns its handle, or -1.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_create(w: i64, h: i64) -> i64 {
    if w <= 0 || h <= 0 {
        return -1;
    }
    let Some(pm) = Pixmap::new(w as u32, h as u32) else {
        return -1;
    };
    let id = next_id();
    surfaces().lock().unwrap().insert(id, pm);
    id
}

/// Destroys a surface, freeing its pixel buffer. Idempotent.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_destroy(s: i64) {
    surfaces().lock().unwrap().remove(&s);
}

/// Returns the surface width in pixels, or -1 if the handle is unknown.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_width(s: i64) -> i64 {
    surfaces()
        .lock()
        .unwrap()
        .get(&s)
        .map_or(-1, |pm| pm.width() as i64)
}

/// Returns the surface height in pixels, or -1 if the handle is unknown.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_height(s: i64) -> i64 {
    surfaces()
        .lock()
        .unwrap()
        .get(&s)
        .map_or(-1, |pm| pm.height() as i64)
}

/// Encodes the surface as PNG into the shared encode cell, returning the byte
/// length (read back via `elephc_img_encoded_ptr`/`_len`), or -1 on failure.
#[no_mangle]
pub extern "C" fn elephc_cairo_surface_encode_png(s: i64) -> i64 {
    let guard = surfaces().lock().unwrap();
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
}

/// Writes the surface to a PNG file at `path`. Returns 0 on success, -1 on error.
///
/// # Safety
/// `path` must be a valid NUL-terminated C string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_cairo_surface_write_png(s: i64, path: *const c_char) -> i64 {
    let Some(path) = cstr_arg(path) else {
        return -1;
    };
    let guard = surfaces().lock().unwrap();
    let Some(pm) = guard.get(&s) else {
        return -1;
    };
    pm.save_png(path).map(|_| 0).unwrap_or(-1)
}

/// Decodes a PNG file into a new image surface, premultiplying its alpha into the
/// pixel buffer the way tiny-skia's `Pixmap` stores it. Returns the surface handle,
/// or -1 if the file is missing/undecodable or the dimensions are invalid.
///
/// # Safety
/// `path` must be a valid NUL-terminated C string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_cairo_surface_create_from_png(path: *const c_char) -> i64 {
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
    surfaces().lock().unwrap().insert(id, pm);
    id
}

/// Creates a drawing context targeting `surface`. Returns its handle, or -1 if the
/// surface is unknown.
#[no_mangle]
pub extern "C" fn elephc_cairo_create(surface: i64) -> i64 {
    if !surfaces().lock().unwrap().contains_key(&surface) {
        return -1;
    }
    let ctx = CairoCtx {
        surface,
        pb: PathBuilder::new(),
        has_current: false,
        cur: Point::from_xy(0.0, 0.0),
        sub_start: Point::from_xy(0.0, 0.0),
        source: CairoSource::Solid([0, 0, 0, 255]),
        line_width: 2.0,
        line_cap: LineCap::Butt,
        line_join: LineJoin::Miter,
        fill_rule: FillRule::Winding,
        ctm: Transform::identity(),
        stack: Vec::new(),
    };
    let id = next_id();
    contexts().lock().unwrap().insert(id, ctx);
    id
}

/// Destroys a drawing context. Idempotent. The target surface is unaffected.
#[no_mangle]
pub extern "C" fn elephc_cairo_destroy(ctx: i64) {
    contexts().lock().unwrap().remove(&ctx);
}

/// Clones a source for the save stack (shaders are `Clone`).
fn clone_source(src: &CairoSource) -> CairoSource {
    match src {
        CairoSource::Solid(c) => CairoSource::Solid(*c),
        CairoSource::Shader(s) => CairoSource::Shader(s.clone()),
    }
}

/// Pushes the current drawing state onto the save stack.
#[no_mangle]
pub extern "C" fn elephc_cairo_save(ctx: i64) {
    let mut guard = contexts().lock().unwrap();
    let Some(c) = guard.get_mut(&ctx) else {
        return;
    };
    let saved = SavedState {
        source: clone_source(&c.source),
        line_width: c.line_width,
        line_cap: c.line_cap,
        line_join: c.line_join,
        fill_rule: c.fill_rule,
        ctm: c.ctm,
    };
    c.stack.push(saved);
}

/// Restores the most recently saved drawing state (no-op if the stack is empty).
#[no_mangle]
pub extern "C" fn elephc_cairo_restore(ctx: i64) {
    let mut guard = contexts().lock().unwrap();
    let Some(c) = guard.get_mut(&ctx) else {
        return;
    };
    if let Some(s) = c.stack.pop() {
        c.source = s.source;
        c.line_width = s.line_width;
        c.line_cap = s.line_cap;
        c.line_join = s.line_join;
        c.fill_rule = s.fill_rule;
        c.ctm = s.ctm;
    }
}

/// Sets the paint source to a solid RGBA color.
#[no_mangle]
pub extern "C" fn elephc_cairo_set_source_rgba(ctx: i64, packed: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.source = CairoSource::Solid(rgba(packed));
    }
}

/// Sets the line width (fixed-point milli user-space units).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_line_width(ctx: i64, w: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.line_width = (w as f64) / 1000.0;
    }
}

/// Sets the line cap style (0 = butt, 1 = round, 2 = square).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_line_cap(ctx: i64, cap: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.line_cap = match cap {
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => LineCap::Butt,
        };
    }
}

/// Sets the line join style (0 = miter, 1 = round, 2 = bevel).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_line_join(ctx: i64, join: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.line_join = match join {
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
    }
}

/// Sets the fill rule (0 = winding/nonzero, 1 = even-odd).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_fill_rule(ctx: i64, rule: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.fill_rule = if rule == 1 {
            FillRule::EvenOdd
        } else {
            FillRule::Winding
        };
    }
}

/// Begins a new sub-path at the given user-space point.
#[no_mangle]
pub extern "C" fn elephc_cairo_move_to(ctx: i64, p_xy: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let d = to_device(c, pt(p_xy));
        c.pb.move_to(d.x, d.y);
        c.cur = d;
        c.sub_start = d;
        c.has_current = true;
    }
}

/// Adds a line from the current point to the given user-space point.
#[no_mangle]
pub extern "C" fn elephc_cairo_line_to(ctx: i64, p_xy: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let d = to_device(c, pt(p_xy));
        if !c.has_current {
            c.pb.move_to(d.x, d.y);
            c.sub_start = d;
        } else {
            c.pb.line_to(d.x, d.y);
        }
        c.cur = d;
        c.has_current = true;
    }
}

/// Adds a cubic Bézier curve through two control points to an end point (all
/// user-space, packed pairs).
#[no_mangle]
pub extern "C" fn elephc_cairo_curve_to(ctx: i64, p1: i64, p2: i64, p3: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let d1 = to_device(c, pt(p1));
        let d2 = to_device(c, pt(p2));
        let d3 = to_device(c, pt(p3));
        if !c.has_current {
            c.pb.move_to(d1.x, d1.y);
            c.sub_start = d1;
        }
        c.pb.cubic_to(d1.x, d1.y, d2.x, d2.y, d3.x, d3.y);
        c.cur = d3;
        c.has_current = true;
    }
}

/// Adds an axis-aligned rectangle sub-path at the given user-space origin/size.
#[no_mangle]
pub extern "C" fn elephc_cairo_rectangle(ctx: i64, p_xy: i64, p_wh: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let (x, y) = unpack_pair(p_xy);
        let (w, h) = unpack_pair(p_wh);
        let p0 = Point::from_xy(fx(x), fx(y));
        let p1 = Point::from_xy(fx(x + w), fx(y));
        let p2 = Point::from_xy(fx(x + w), fx(y + h));
        let p3 = Point::from_xy(fx(x), fx(y + h));
        let d0 = to_device(c, p0);
        c.pb.move_to(d0.x, d0.y);
        for p in [p1, p2, p3] {
            let d = to_device(c, p);
            c.pb.line_to(d.x, d.y);
        }
        c.pb.close();
        c.cur = d0;
        c.sub_start = d0;
        c.has_current = true;
    }
}

/// Samples an arc (center, radius, start/end angle in milli-radians) into line
/// segments, appended to the current path. `negative` reverses the sweep.
fn append_arc(c: &mut CairoCtx, p_center: i64, radius_fx: i64, p_angles: i64, negative: bool) {
    let (cx, cy) = unpack_pair(p_center);
    let center = Point::from_xy(fx(cx), fx(cy));
    let r = fx(radius_fx);
    let (a1i, a2i) = unpack_pair(p_angles);
    let a1 = (a1i as f32) / 1000.0;
    let mut a2 = (a2i as f32) / 1000.0;
    // Normalize sweep direction the way Cairo does for arc / arc_negative.
    if !negative {
        while a2 < a1 {
            a2 += std::f32::consts::TAU;
        }
    } else {
        while a2 > a1 {
            a2 -= std::f32::consts::TAU;
        }
    }
    let sweep = (a2 - a1).abs();
    let steps = ((sweep / (std::f32::consts::PI / 32.0)).ceil() as i32).max(2);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = a1 + (a2 - a1) * t;
        let p = Point::from_xy(center.x + r * a.cos(), center.y + r * a.sin());
        let d = to_device(c, p);
        if i == 0 && !c.has_current {
            c.pb.move_to(d.x, d.y);
            c.sub_start = d;
        } else if i == 0 {
            c.pb.line_to(d.x, d.y);
        } else {
            c.pb.line_to(d.x, d.y);
        }
        c.cur = d;
    }
    c.has_current = true;
}

/// Adds a clockwise arc to the current path.
#[no_mangle]
pub extern "C" fn elephc_cairo_arc(ctx: i64, p_center: i64, radius_fx: i64, p_angles: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        append_arc(c, p_center, radius_fx, p_angles, false);
    }
}

/// Adds a counter-clockwise arc to the current path.
#[no_mangle]
pub extern "C" fn elephc_cairo_arc_negative(ctx: i64, p_center: i64, radius_fx: i64, p_angles: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        append_arc(c, p_center, radius_fx, p_angles, true);
    }
}

/// Closes the current sub-path back to its start point.
#[no_mangle]
pub extern "C" fn elephc_cairo_close_path(ctx: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        if c.has_current {
            c.pb.close();
            c.cur = c.sub_start;
        }
    }
}

/// Discards the current path, leaving no current point.
#[no_mangle]
pub extern "C" fn elephc_cairo_new_path(ctx: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.pb = PathBuilder::new();
        c.has_current = false;
    }
}

/// Begins a new sub-path without a current point (next move/line starts fresh).
#[no_mangle]
pub extern "C" fn elephc_cairo_new_sub_path(ctx: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.has_current = false;
    }
}

/// Translates the current transformation matrix by a user-space offset.
#[no_mangle]
pub extern "C" fn elephc_cairo_translate(ctx: i64, p_xy: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let (x, y) = unpack_pair(p_xy);
        c.ctm = c.ctm.pre_concat(Transform::from_translate(fx(x), fx(y)));
    }
}

/// Scales the current transformation matrix (fixed-point milli factors).
#[no_mangle]
pub extern "C" fn elephc_cairo_scale(ctx: i64, p_sxsy: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let (sx, sy) = unpack_pair(p_sxsy);
        c.ctm = c.ctm.pre_concat(Transform::from_scale(fx(sx), fx(sy)));
    }
}

/// Rotates the current transformation matrix by `angle` milli-radians.
#[no_mangle]
pub extern "C" fn elephc_cairo_rotate(ctx: i64, angle_mrad: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let degrees = (angle_mrad as f32 / 1000.0).to_degrees();
        c.ctm = c.ctm.pre_concat(Transform::from_rotate(degrees));
    }
}

/// Replaces the current transformation matrix (row-major a,b,c,d,e,f as Cairo
/// orders them, packed into three pairs).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_matrix(ctx: i64, p_ab: i64, p_cd: i64, p_ef: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let (a, b) = unpack_pair(p_ab);
        let (cc, d) = unpack_pair(p_cd);
        let (e, f) = unpack_pair(p_ef);
        // Cairo matrix (xx=a, yx=b, xy=c, yy=d, x0=e, y0=f) maps to tiny-skia's
        // from_row(sx=a, ky=b, kx=c, sy=d, tx=e, ty=f).
        c.ctm = Transform::from_row(fx(a), fx(b), fx(cc), fx(d), fx(e), fx(f));
    }
}

/// Composes the given matrix (Cairo a,b,c,d,e,f, packed into three pairs) onto the
/// current transformation matrix.
#[no_mangle]
pub extern "C" fn elephc_cairo_transform(ctx: i64, p_ab: i64, p_cd: i64, p_ef: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        let (a, b) = unpack_pair(p_ab);
        let (cc, d) = unpack_pair(p_cd);
        let (e, f) = unpack_pair(p_ef);
        let m = Transform::from_row(fx(a), fx(b), fx(cc), fx(d), fx(e), fx(f));
        c.ctm = c.ctm.pre_concat(m);
    }
}

/// Resets the current transformation matrix to the identity.
#[no_mangle]
pub extern "C" fn elephc_cairo_identity_matrix(ctx: i64) {
    if let Some(c) = contexts().lock().unwrap().get_mut(&ctx) {
        c.ctm = Transform::identity();
    }
}

/// Returns the current point's device-space x in fixed-point milli units, or 0.
#[no_mangle]
pub extern "C" fn elephc_cairo_get_current_point_x(ctx: i64) -> i64 {
    contexts()
        .lock()
        .unwrap()
        .get(&ctx)
        .map_or(0, |c| (c.cur.x * 1000.0).round() as i64)
}

/// Returns the current point's device-space y in fixed-point milli units, or 0.
#[no_mangle]
pub extern "C" fn elephc_cairo_get_current_point_y(ctx: i64) -> i64 {
    contexts()
        .lock()
        .unwrap()
        .get(&ctx)
        .map_or(0, |c| (c.cur.y * 1000.0).round() as i64)
}

/// Builds a `Paint` for the current source.
fn make_paint(src: &CairoSource) -> Paint<'_> {
    let mut paint = Paint::default();
    paint.anti_alias = true;
    match src {
        CairoSource::Solid(c) => paint.set_color_rgba8(c[0], c[1], c[2], c[3]),
        CairoSource::Shader(s) => paint.shader = s.clone(),
    }
    paint
}

/// Fills the entire clip region (here the whole surface) with the current source.
#[no_mangle]
pub extern "C" fn elephc_cairo_paint(ctx: i64) {
    let mut cguard = contexts().lock().unwrap();
    let Some(c) = cguard.get_mut(&ctx) else {
        return;
    };
    let surface = c.surface;
    let paint = make_paint(&c.source);
    let mut sguard = surfaces().lock().unwrap();
    if let Some(pm) = sguard.get_mut(&surface) {
        let (w, h) = (pm.width() as f32, pm.height() as f32);
        if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
            pm.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }
}

/// Rasterizes the current path. `stroke` selects stroke vs fill; `preserve` keeps
/// the path afterwards instead of clearing it.
fn raster(ctx: i64, stroke: bool, preserve: bool) {
    let mut cguard = contexts().lock().unwrap();
    let Some(c) = cguard.get_mut(&ctx) else {
        return;
    };
    let path: Option<Path> = c.pb.clone().finish();
    let surface = c.surface;
    let paint = make_paint(&c.source);
    let stroke_spec = Stroke {
        width: (c.line_width * matrix_scale(&c.ctm)) as f32,
        line_cap: c.line_cap,
        line_join: c.line_join,
        ..Stroke::default()
    };
    let fill_rule = c.fill_rule;
    if let Some(path) = path {
        let mut sguard = surfaces().lock().unwrap();
        if let Some(pm) = sguard.get_mut(&surface) {
            if stroke {
                pm.stroke_path(&path, &paint, &stroke_spec, Transform::identity(), None);
            } else {
                pm.fill_path(&path, &paint, fill_rule, Transform::identity(), None);
            }
        }
    }
    if !preserve {
        c.pb = PathBuilder::new();
        c.has_current = false;
    }
}

/// Fills the current path with the current source, then clears the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_fill(ctx: i64) {
    raster(ctx, false, false);
}

/// Fills the current path with the current source, keeping the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_fill_preserve(ctx: i64) {
    raster(ctx, false, true);
}

/// Strokes the current path with the current source, then clears the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_stroke(ctx: i64) {
    raster(ctx, true, false);
}

/// Strokes the current path with the current source, keeping the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_stroke_preserve(ctx: i64) {
    raster(ctx, true, true);
}

/// Creates a solid-color pattern. Returns its handle.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_create_rgba(packed: i64) -> i64 {
    let p = CairoPattern {
        kind: CairoPatternKind::Solid(rgba(packed)),
        stops: Vec::new(),
    };
    let id = next_id();
    patterns().lock().unwrap().insert(id, p);
    id
}

/// Creates a linear-gradient pattern between two user-space points.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_create_linear(p0: i64, p1: i64) -> i64 {
    let p = CairoPattern {
        kind: CairoPatternKind::Linear {
            p0: pt(p0),
            p1: pt(p1),
        },
        stops: Vec::new(),
    };
    let id = next_id();
    patterns().lock().unwrap().insert(id, p);
    id
}

/// Creates a radial-gradient pattern. tiny-skia uses a single end circle, so the
/// outer circle (`c1`, `r1`) is used; the inner radius is approximated as a stop.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_create_radial(_p_c0: i64, _r0_fx: i64, p_c1: i64, r1_fx: i64) -> i64 {
    let p = CairoPattern {
        kind: CairoPatternKind::Radial {
            c1: pt(p_c1),
            r1: fx(r1_fx),
        },
        stops: Vec::new(),
    };
    let id = next_id();
    patterns().lock().unwrap().insert(id, p);
    id
}

/// Adds a color stop (offset in fixed-point milli 0..1000) to a gradient pattern.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_add_color_stop_rgba(pattern: i64, offset_fx: i64, packed: i64) {
    if let Some(p) = patterns().lock().unwrap().get_mut(&pattern) {
        let offset = (offset_fx as f32 / 1000.0).clamp(0.0, 1.0);
        p.stops.push((offset, color(packed)));
    }
}

/// Destroys a pattern. Idempotent.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_destroy(pattern: i64) {
    patterns().lock().unwrap().remove(&pattern);
}

/// Sets the context's source to a pattern, building the shader in device space
/// using the current matrix. A solid pattern (or a gradient that fails to build)
/// falls back to a solid color.
#[no_mangle]
pub extern "C" fn elephc_cairo_set_source_pattern(ctx: i64, pattern: i64) {
    let mut cguard = contexts().lock().unwrap();
    let Some(c) = cguard.get_mut(&ctx) else {
        return;
    };
    let pguard = patterns().lock().unwrap();
    let Some(p) = pguard.get(&pattern) else {
        return;
    };
    let stops: Vec<GradientStop> = p
        .stops
        .iter()
        .map(|(o, col)| GradientStop::new(*o, *col))
        .collect();
    c.source = match &p.kind {
        CairoPatternKind::Solid(col) => CairoSource::Solid(*col),
        CairoPatternKind::Linear { p0, p1 } => {
            let start = to_device(c, *p0);
            let end = to_device(c, *p1);
            match LinearGradient::new(start, end, stops, SpreadMode::Pad, Transform::identity()) {
                Some(sh) => CairoSource::Shader(sh),
                None => CairoSource::Solid([0, 0, 0, 255]),
            }
        }
        CairoPatternKind::Radial { c1, r1 } => {
            let center = to_device(c, *c1);
            let radius = r1 * matrix_scale(&c.ctm) as f32;
            match RadialGradient::new(
                center,
                center,
                radius.max(0.01),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            ) {
                Some(sh) => CairoSource::Shader(sh),
                None => CairoSource::Solid([0, 0, 0, 255]),
            }
        }
    };
}
