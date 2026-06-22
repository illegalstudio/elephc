//! Purpose:
//! Pure-Rust Cairo bridge: backs the PHP `Cairo*` object API on `tiny-skia`. This
//! module owns the shared state for the bridge — an image-surface table (each a
//! `tiny_skia::Pixmap`), a drawing-context table (path builder, source paint, line
//! state, transformation matrix, save/restore stack), and a pattern table (solid /
//! linear / radial gradient specs) — plus the shared coordinate/color helpers. Paths
//! are accumulated in device space (each point mapped through the current matrix as it
//! is added), so fill/stroke raster with the identity transform. The C ABI entry
//! points are split across the `surface`, `context`, and `pattern` submodules.
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
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, PathBuilder, Pixmap, Point, Shader, Transform,
};

use crate::unpack_pair;

mod context;
mod pattern;
mod surface;

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
