//! Purpose:
//! ImagickDraw command-buffer object table and the `Imagick::drawImage` renderer.
//! An ImagickDraw accumulates fill/stroke state and primitive commands (line,
//! rectangle, circle, ellipse, point, polygon); each command captures the
//! fill/stroke/width active when it was added. Rendering replays the buffer onto a
//! wand's current frame through the existing GD drawing entry points.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind the `ImagickDraw` methods and `Imagick::drawImage`.
//!
//! Key details:
//! - Keeping the command buffer in the bridge (rather than as PHP arrays) avoids
//!   marshaling heterogeneous draw data across the C ABI and reuses every GD
//!   rasterizer (`elephc_img_*`) for the actual pixels.
//! - The stroke color uses `-1` as a "no stroke" sentinel (valid GD packed colors
//!   are non-negative), so a shape with only a fill draws no outline.
//! - Filled shapes are drawn with the captured fill color first, then outlined
//!   with the stroke color, matching ImagickDraw's fill-then-stroke model.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::draw::{
    elephc_img_arc, elephc_img_ellipse, elephc_img_filled_arc, elephc_img_filled_ellipse,
    elephc_img_filled_rectangle, elephc_img_line, elephc_img_poly_add, elephc_img_poly_fill,
    elephc_img_poly_line, elephc_img_poly_reset, elephc_img_rectangle, elephc_img_set_thickness,
};
use crate::{ffi_guard, lock_recover};
use crate::gd::elephc_img_set_pixel;
use crate::imagick::current_handle;

/// Sentinel packed-color value meaning "no color set" (no stroke / no fill).
const NO_COLOR: i64 = -1;

/// A single buffered draw primitive plus the fill/stroke/width captured when it
/// was recorded.
struct Cmd {
    kind: CmdKind,
    fill: i64,
    stroke: i64,
    width: i64,
}

/// The geometry of a buffered draw primitive. Angles are in millidegrees.
enum CmdKind {
    Line(i64, i64, i64, i64),
    Rectangle(i64, i64, i64, i64),
    Ellipse {
        cx: i64,
        cy: i64,
        rx: i64,
        ry: i64,
        start: i64,
        end: i64,
    },
    Point(i64, i64),
    Polygon(Vec<(i64, i64)>),
}

/// A live ImagickDraw: current fill/stroke/width, the recorded command list, and
/// the polygon-point scratch buffer used while a polygon is being described.
struct DrawState {
    fill: i64,
    stroke: i64,
    width: i64,
    cmds: Vec<Cmd>,
    poly_buf: Vec<(i64, i64)>,
}

/// Global table of live ImagickDraw objects keyed by opaque draw ID.
fn draws() -> &'static Mutex<HashMap<i64, DrawState>> {
    static DRAWS: OnceLock<Mutex<HashMap<i64, DrawState>>> = OnceLock::new();
    DRAWS.get_or_init(Mutex::default)
}

/// Returns a fresh, never-reused ImagickDraw ID (independent counter; `0`/`-1`
/// stay free as sentinels).
fn next_draw_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Packs two 32-bit values into one `i64` as `(hi << 32) | lo`, matching the
/// `unpack_pair` layout the GD arc entry points expect.
fn pack_pair(hi: i64, lo: i64) -> i64 {
    ((hi & 0xffff_ffff) << 32) | (lo & 0xffff_ffff)
}

/// Records a command on a draw object, snapshotting its current fill/stroke/width.
fn push_cmd(draw_id: i64, kind: CmdKind) {
    if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
        state.cmds.push(Cmd {
            kind,
            fill: state.fill,
            stroke: state.stroke,
            width: state.width,
        });
    }
}

/// Creates a new ImagickDraw and returns its handle. Default state: opaque black
/// fill, no stroke, stroke width 1.
#[no_mangle]
pub extern "C" fn elephc_idraw_new() -> i64 {
    ffi_guard(-1, move || {
        let id = next_draw_id();
        lock_recover(draws()).insert(
            id,
            DrawState {
                fill: 0,
                stroke: NO_COLOR,
                width: 1,
                cmds: Vec::new(),
                poly_buf: Vec::new(),
            },
        );
        id
    })
}

/// Destroys an ImagickDraw, releasing its buffer. Idempotent. Backs
/// `ImagickDraw::destroy` and `__destruct`.
#[no_mangle]
pub extern "C" fn elephc_idraw_destroy(draw_id: i64) {
    ffi_guard((), move || {
        lock_recover(draws()).remove(&draw_id);
    })
}

/// Clears all recorded commands and resets state to defaults. Backs
/// `ImagickDraw::clear`.
#[no_mangle]
pub extern "C" fn elephc_idraw_clear(draw_id: i64) {
    ffi_guard((), move || {
        if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
            state.fill = 0;
            state.stroke = NO_COLOR;
            state.width = 1;
            state.cmds.clear();
            state.poly_buf.clear();
        }
    })
}

/// Sets the current fill color (GD packed). Backs `ImagickDraw::setFillColor`.
#[no_mangle]
pub extern "C" fn elephc_idraw_set_fill(draw_id: i64, color: i64) {
    ffi_guard((), move || {
        if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
            state.fill = color;
        }
    })
}

/// Sets the current stroke color (GD packed, or `-1` for none). Backs
/// `ImagickDraw::setStrokeColor`.
#[no_mangle]
pub extern "C" fn elephc_idraw_set_stroke(draw_id: i64, color: i64) {
    ffi_guard((), move || {
        if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
            state.stroke = color;
        }
    })
}

/// Sets the current stroke width in pixels (clamped to at least 1). Backs
/// `ImagickDraw::setStrokeWidth`.
#[no_mangle]
pub extern "C" fn elephc_idraw_set_stroke_width(draw_id: i64, width: i64) {
    ffi_guard((), move || {
        if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
            state.width = width.max(1);
        }
    })
}

/// Returns the current fill color, or `-1` for an unknown draw. Backs
/// `ImagickDraw::getFillColor` (the PHP layer wraps it in an ImagickPixel).
#[no_mangle]
pub extern "C" fn elephc_idraw_get_fill(draw_id: i64) -> i64 {
    ffi_guard(-1, move || {
        match lock_recover(draws()).get(&draw_id) {
            Some(state) => state.fill,
            None => NO_COLOR,
        }
    })
}

/// Records a line from `(x1, y1)` to `(x2, y2)`. Backs `ImagickDraw::line`.
#[no_mangle]
pub extern "C" fn elephc_idraw_line(draw_id: i64, x1: i64, y1: i64, x2: i64, y2: i64) {
    ffi_guard((), move || {
        push_cmd(draw_id, CmdKind::Line(x1, y1, x2, y2));
    })
}

/// Records a rectangle through opposite corners `(x1, y1)`-`(x2, y2)`. Backs
/// `ImagickDraw::rectangle`.
#[no_mangle]
pub extern "C" fn elephc_idraw_rectangle(draw_id: i64, x1: i64, y1: i64, x2: i64, y2: i64) {
    ffi_guard((), move || {
        push_cmd(draw_id, CmdKind::Rectangle(x1, y1, x2, y2));
    })
}

/// Records a full circle centered at `(ox, oy)` whose radius is the distance to
/// the perimeter point `(px, py)`, matching `ImagickDraw::circle`'s argument form.
#[no_mangle]
pub extern "C" fn elephc_idraw_circle(draw_id: i64, ox: i64, oy: i64, px: i64, py: i64) {
    ffi_guard((), move || {
        let dx = (px - ox) as f64;
        let dy = (py - oy) as f64;
        let r = (dx * dx + dy * dy).sqrt().round() as i64;
        push_cmd(
            draw_id,
            CmdKind::Ellipse {
                cx: ox,
                cy: oy,
                rx: r,
                ry: r,
                start: 0,
                end: 360_000,
            },
        );
    })
}

/// Records an ellipse/arc centered at the packed `oxy` with packed radii `rxy` and
/// the packed start/end angle in degrees `se`. Backs `ImagickDraw::ellipse`
/// (a full ellipse uses `0`-`360`).
#[no_mangle]
pub extern "C" fn elephc_idraw_ellipse(draw_id: i64, oxy: i64, rxy: i64, se: i64) {
    ffi_guard((), move || {
        let (ox, oy) = crate::unpack_pair(oxy);
        let (rx, ry) = crate::unpack_pair(rxy);
        let (start, end) = crate::unpack_pair(se);
        push_cmd(
            draw_id,
            CmdKind::Ellipse {
                cx: ox,
                cy: oy,
                rx,
                ry,
                start: start * 1000,
                end: end * 1000,
            },
        );
    })
}

/// Records a single point at `(x, y)`. Backs `ImagickDraw::point`.
#[no_mangle]
pub extern "C" fn elephc_idraw_point(draw_id: i64, x: i64, y: i64) {
    ffi_guard((), move || {
        push_cmd(draw_id, CmdKind::Point(x, y));
    })
}

/// Clears the polygon-point scratch buffer before a new polygon is described.
#[no_mangle]
pub extern "C" fn elephc_idraw_poly_reset(draw_id: i64) {
    ffi_guard((), move || {
        if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
            state.poly_buf.clear();
        }
    })
}

/// Appends a vertex to the polygon-point scratch buffer.
#[no_mangle]
pub extern "C" fn elephc_idraw_poly_point(draw_id: i64, x: i64, y: i64) {
    ffi_guard((), move || {
        if let Some(state) = lock_recover(draws()).get_mut(&draw_id) {
            state.poly_buf.push((x, y));
        }
    })
}

/// Commits the buffered polygon points as a polygon command. Backs
/// `ImagickDraw::polygon`.
#[no_mangle]
pub extern "C" fn elephc_idraw_polygon(draw_id: i64) {
    ffi_guard((), move || {
        let pts = match lock_recover(draws()).get(&draw_id) {
            Some(state) => state.poly_buf.clone(),
            None => return,
        };
        push_cmd(draw_id, CmdKind::Polygon(pts));
    })
}

/// Replays an ImagickDraw's command list onto the current frame of `wand_id`.
/// Returns `0` on success and `-1` for an unknown wand/draw or an empty wand.
/// Backs `Imagick::drawImage`.
#[no_mangle]
pub extern "C" fn elephc_imagick_draw(wand_id: i64, draw_id: i64) -> i64 {
    ffi_guard(-1, move || {
        let Some(handle) = current_handle(wand_id) else {
            return -1;
        };
        // Snapshot the command list so the draws() lock is not held during rendering
        // (the GD entry points take the images() lock, never draws()).
        let cmds: Vec<(CmdSnapshot, i64, i64, i64)> = {
            let guard = lock_recover(draws());
            let Some(state) = guard.get(&draw_id) else {
                return -1;
            };
            state
                .cmds
                .iter()
                .map(|c| (CmdSnapshot::from(&c.kind), c.fill, c.stroke, c.width))
                .collect()
        };
        for (kind, fill, stroke, width) in cmds {
            elephc_img_set_thickness(handle, width);
            render_one(handle, &kind, fill, stroke);
        }
        0
    })
}

/// A render-time copy of a command's geometry, decoupled from the draws() table.
enum CmdSnapshot {
    Line(i64, i64, i64, i64),
    Rectangle(i64, i64, i64, i64),
    Ellipse {
        cx: i64,
        cy: i64,
        rx: i64,
        ry: i64,
        start: i64,
        end: i64,
    },
    Point(i64, i64),
    Polygon(Vec<(i64, i64)>),
}

impl From<&CmdKind> for CmdSnapshot {
    /// Clones a buffered command's geometry into a lock-free snapshot for render.
    fn from(kind: &CmdKind) -> Self {
        match kind {
            CmdKind::Line(a, b, c, d) => CmdSnapshot::Line(*a, *b, *c, *d),
            CmdKind::Rectangle(a, b, c, d) => CmdSnapshot::Rectangle(*a, *b, *c, *d),
            CmdKind::Ellipse {
                cx,
                cy,
                rx,
                ry,
                start,
                end,
            } => CmdSnapshot::Ellipse {
                cx: *cx,
                cy: *cy,
                rx: *rx,
                ry: *ry,
                start: *start,
                end: *end,
            },
            CmdKind::Point(x, y) => CmdSnapshot::Point(*x, *y),
            CmdKind::Polygon(pts) => CmdSnapshot::Polygon(pts.clone()),
        }
    }
}

/// Rasterizes one snapshotted command onto `handle`, filling with `fill` then
/// outlining with `stroke` (each skipped when its color is the `-1` sentinel).
fn render_one(handle: i64, kind: &CmdSnapshot, fill: i64, stroke: i64) {
    // Lines/points have no interior: prefer the stroke color, else the fill.
    let line_color = if stroke != NO_COLOR { stroke } else { fill };
    match kind {
        CmdSnapshot::Line(x1, y1, x2, y2) => {
            elephc_img_line(handle, *x1, *y1, *x2, *y2, line_color);
        }
        CmdSnapshot::Point(x, y) => {
            elephc_img_set_pixel(handle, *x, *y, line_color);
        }
        CmdSnapshot::Rectangle(x1, y1, x2, y2) => {
            if fill != NO_COLOR {
                elephc_img_filled_rectangle(handle, *x1, *y1, *x2, *y2, fill);
            }
            if stroke != NO_COLOR {
                elephc_img_rectangle(handle, *x1, *y1, *x2, *y2, stroke);
            }
        }
        CmdSnapshot::Ellipse {
            cx,
            cy,
            rx,
            ry,
            start,
            end,
        } => {
            let (w, h) = (rx * 2, ry * 2);
            let full = end - start >= 360_000;
            if full {
                if fill != NO_COLOR {
                    elephc_img_filled_ellipse(handle, *cx, *cy, w, h, fill);
                }
                if stroke != NO_COLOR {
                    elephc_img_ellipse(handle, *cx, *cy, w, h, stroke);
                }
            } else {
                let cxy = pack_pair(*cx, *cy);
                let wh = pack_pair(w, h);
                let (sd, ed) = (start / 1000, end / 1000);
                if fill != NO_COLOR {
                    elephc_img_filled_arc(handle, cxy, wh, sd, ed, fill);
                }
                if stroke != NO_COLOR {
                    elephc_img_arc(handle, cxy, wh, sd, ed, stroke);
                }
            }
        }
        CmdSnapshot::Polygon(pts) => {
            elephc_img_poly_reset();
            for (x, y) in pts {
                elephc_img_poly_add(*x, *y);
            }
            if fill != NO_COLOR {
                elephc_img_poly_fill(handle, fill);
            }
            if stroke != NO_COLOR {
                elephc_img_poly_line(handle, stroke, 1);
            }
        }
    }
}
