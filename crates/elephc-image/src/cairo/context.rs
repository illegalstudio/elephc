//! Purpose:
//! Drawing-context, path, transform, and raster C ABI entry points for the Cairo
//! bridge: create / destroy contexts, save/restore state, set source color and line /
//! fill state, build paths (move/line/curve/rectangle/arc/close), manipulate the
//! transformation matrix, query the current point, and paint / fill / stroke. Also
//! holds the context-only helpers `clone_source`, `append_arc`, `make_paint`, and
//! `raster`.
//!
//! Called from:
//! - the image prelude's `extern "elephc_image"` block (`CairoContext`).
//!
//! Key details:
//! - Path points are mapped through the current matrix into device space as they are
//!   added, so raster runs with the identity transform. Line width is user-space and
//!   scaled by the matrix's geometric-mean scale at stroke time. Arcs are sampled into
//!   line segments.

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Point, Stroke, Transform,
};

use super::{
    contexts, fx, matrix_scale, next_id, pt, rgba, surfaces, to_device, CairoCtx, CairoSource,
    SavedState,
};
use crate::{ffi_guard, lock_recover, unpack_pair};

/// Creates a drawing context targeting `surface`. Returns its handle, or -1 if the
/// surface is unknown.
#[no_mangle]
pub extern "C" fn elephc_cairo_create(surface: i64) -> i64 {
    ffi_guard(-1, move || {
        if !lock_recover(surfaces()).contains_key(&surface) {
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
        lock_recover(contexts()).insert(id, ctx);
        id
    })
}

/// Destroys a drawing context. Idempotent. The target surface is unaffected.
#[no_mangle]
pub extern "C" fn elephc_cairo_destroy(ctx: i64) {
    ffi_guard((), move || {
        lock_recover(contexts()).remove(&ctx);
    })
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
    ffi_guard((), move || {
        let mut guard = lock_recover(contexts());
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
    })
}

/// Restores the most recently saved drawing state (no-op if the stack is empty).
#[no_mangle]
pub extern "C" fn elephc_cairo_restore(ctx: i64) {
    ffi_guard((), move || {
        let mut guard = lock_recover(contexts());
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
    })
}

/// Sets the paint source to a solid RGBA color.
#[no_mangle]
pub extern "C" fn elephc_cairo_set_source_rgba(ctx: i64, packed: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.source = CairoSource::Solid(rgba(packed));
        }
    })
}

/// Sets the line width (fixed-point milli user-space units).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_line_width(ctx: i64, w: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.line_width = (w as f64) / 1000.0;
        }
    })
}

/// Sets the line cap style (0 = butt, 1 = round, 2 = square).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_line_cap(ctx: i64, cap: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.line_cap = match cap {
                1 => LineCap::Round,
                2 => LineCap::Square,
                _ => LineCap::Butt,
            };
        }
    })
}

/// Sets the line join style (0 = miter, 1 = round, 2 = bevel).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_line_join(ctx: i64, join: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.line_join = match join {
                1 => LineJoin::Round,
                2 => LineJoin::Bevel,
                _ => LineJoin::Miter,
            };
        }
    })
}

/// Sets the fill rule (0 = winding/nonzero, 1 = even-odd).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_fill_rule(ctx: i64, rule: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.fill_rule = if rule == 1 {
                FillRule::EvenOdd
            } else {
                FillRule::Winding
            };
        }
    })
}

/// Begins a new sub-path at the given user-space point.
#[no_mangle]
pub extern "C" fn elephc_cairo_move_to(ctx: i64, p_xy: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            let d = to_device(c, pt(p_xy));
            c.pb.move_to(d.x, d.y);
            c.cur = d;
            c.sub_start = d;
            c.has_current = true;
        }
    })
}

/// Adds a line from the current point to the given user-space point.
#[no_mangle]
pub extern "C" fn elephc_cairo_line_to(ctx: i64, p_xy: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
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
    })
}

/// Adds a cubic Bézier curve through two control points to an end point (all
/// user-space, packed pairs).
#[no_mangle]
pub extern "C" fn elephc_cairo_curve_to(ctx: i64, p1: i64, p2: i64, p3: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
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
    })
}

/// Adds an axis-aligned rectangle sub-path at the given user-space origin/size.
#[no_mangle]
pub extern "C" fn elephc_cairo_rectangle(ctx: i64, p_xy: i64, p_wh: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
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
    })
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
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            append_arc(c, p_center, radius_fx, p_angles, false);
        }
    })
}

/// Adds a counter-clockwise arc to the current path.
#[no_mangle]
pub extern "C" fn elephc_cairo_arc_negative(ctx: i64, p_center: i64, radius_fx: i64, p_angles: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            append_arc(c, p_center, radius_fx, p_angles, true);
        }
    })
}

/// Closes the current sub-path back to its start point.
#[no_mangle]
pub extern "C" fn elephc_cairo_close_path(ctx: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            if c.has_current {
                c.pb.close();
                c.cur = c.sub_start;
            }
        }
    })
}

/// Discards the current path, leaving no current point.
#[no_mangle]
pub extern "C" fn elephc_cairo_new_path(ctx: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.pb = PathBuilder::new();
            c.has_current = false;
        }
    })
}

/// Begins a new sub-path without a current point (next move/line starts fresh).
#[no_mangle]
pub extern "C" fn elephc_cairo_new_sub_path(ctx: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.has_current = false;
        }
    })
}

/// Translates the current transformation matrix by a user-space offset.
#[no_mangle]
pub extern "C" fn elephc_cairo_translate(ctx: i64, p_xy: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            let (x, y) = unpack_pair(p_xy);
            c.ctm = c.ctm.pre_concat(Transform::from_translate(fx(x), fx(y)));
        }
    })
}

/// Scales the current transformation matrix (fixed-point milli factors).
#[no_mangle]
pub extern "C" fn elephc_cairo_scale(ctx: i64, p_sxsy: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            let (sx, sy) = unpack_pair(p_sxsy);
            c.ctm = c.ctm.pre_concat(Transform::from_scale(fx(sx), fx(sy)));
        }
    })
}

/// Rotates the current transformation matrix by `angle` milli-radians.
#[no_mangle]
pub extern "C" fn elephc_cairo_rotate(ctx: i64, angle_mrad: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            let degrees = (angle_mrad as f32 / 1000.0).to_degrees();
            c.ctm = c.ctm.pre_concat(Transform::from_rotate(degrees));
        }
    })
}

/// Replaces the current transformation matrix (row-major a,b,c,d,e,f as Cairo
/// orders them, packed into three pairs).
#[no_mangle]
pub extern "C" fn elephc_cairo_set_matrix(ctx: i64, p_ab: i64, p_cd: i64, p_ef: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            let (a, b) = unpack_pair(p_ab);
            let (cc, d) = unpack_pair(p_cd);
            let (e, f) = unpack_pair(p_ef);
            // Cairo matrix (xx=a, yx=b, xy=c, yy=d, x0=e, y0=f) maps to tiny-skia's
            // from_row(sx=a, ky=b, kx=c, sy=d, tx=e, ty=f).
            c.ctm = Transform::from_row(fx(a), fx(b), fx(cc), fx(d), fx(e), fx(f));
        }
    })
}

/// Composes the given matrix (Cairo a,b,c,d,e,f, packed into three pairs) onto the
/// current transformation matrix.
#[no_mangle]
pub extern "C" fn elephc_cairo_transform(ctx: i64, p_ab: i64, p_cd: i64, p_ef: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            let (a, b) = unpack_pair(p_ab);
            let (cc, d) = unpack_pair(p_cd);
            let (e, f) = unpack_pair(p_ef);
            let m = Transform::from_row(fx(a), fx(b), fx(cc), fx(d), fx(e), fx(f));
            c.ctm = c.ctm.pre_concat(m);
        }
    })
}

/// Resets the current transformation matrix to the identity.
#[no_mangle]
pub extern "C" fn elephc_cairo_identity_matrix(ctx: i64) {
    ffi_guard((), move || {
        if let Some(c) = lock_recover(contexts()).get_mut(&ctx) {
            c.ctm = Transform::identity();
        }
    })
}

/// Returns the current point's device-space x in fixed-point milli units, or 0.
#[no_mangle]
pub extern "C" fn elephc_cairo_get_current_point_x(ctx: i64) -> i64 {
    ffi_guard(-1, move || {
        lock_recover(contexts())
            .get(&ctx)
            .map_or(0, |c| (c.cur.x * 1000.0).round() as i64)
    })
}

/// Returns the current point's device-space y in fixed-point milli units, or 0.
#[no_mangle]
pub extern "C" fn elephc_cairo_get_current_point_y(ctx: i64) -> i64 {
    ffi_guard(-1, move || {
        lock_recover(contexts())
            .get(&ctx)
            .map_or(0, |c| (c.cur.y * 1000.0).round() as i64)
    })
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
    ffi_guard((), move || {
        let mut cguard = lock_recover(contexts());
        let Some(c) = cguard.get_mut(&ctx) else {
            return;
        };
        let surface = c.surface;
        let paint = make_paint(&c.source);
        let mut sguard = lock_recover(surfaces());
        if let Some(pm) = sguard.get_mut(&surface) {
            let (w, h) = (pm.width() as f32, pm.height() as f32);
            if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
                pm.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
    })
}

/// Rasterizes the current path. `stroke` selects stroke vs fill; `preserve` keeps
/// the path afterwards instead of clearing it.
fn raster(ctx: i64, stroke: bool, preserve: bool) {
    let mut cguard = lock_recover(contexts());
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
        let mut sguard = lock_recover(surfaces());
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
    ffi_guard((), move || {
        raster(ctx, false, false);
    })
}

/// Fills the current path with the current source, keeping the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_fill_preserve(ctx: i64) {
    ffi_guard((), move || {
        raster(ctx, false, true);
    })
}

/// Strokes the current path with the current source, then clears the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_stroke(ctx: i64) {
    ffi_guard((), move || {
        raster(ctx, true, false);
    })
}

/// Strokes the current path with the current source, keeping the path.
#[no_mangle]
pub extern "C" fn elephc_cairo_stroke_preserve(ctx: i64) {
    ffi_guard((), move || {
        raster(ctx, true, true);
    })
}
