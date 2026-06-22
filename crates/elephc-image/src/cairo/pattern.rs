//! Purpose:
//! Pattern (solid / linear gradient / radial gradient) C ABI entry points for the
//! Cairo bridge: create patterns, append color stops, destroy them, and bind a pattern
//! as a context's paint source by building its shader in device space.
//!
//! Called from:
//! - the image prelude's `extern "elephc_image"` block (`CairoSolidPattern`,
//!   `CairoLinearGradient`, `CairoRadialGradient`).
//!
//! Key details:
//! - Patterns are stored geometry-only in the shared pattern table; the tiny-skia
//!   shader is built lazily at `set_source` using the context's current matrix. A
//!   solid pattern (or a gradient that fails to build) falls back to a solid color.

use tiny_skia::{GradientStop, LinearGradient, RadialGradient, SpreadMode, Transform};

use super::{
    color, contexts, fx, matrix_scale, next_id, patterns, pt, rgba, to_device, CairoPattern,
    CairoPatternKind, CairoSource,
};
use crate::{ffi_guard, lock_recover};

/// Creates a solid-color pattern. Returns its handle.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_create_rgba(packed: i64) -> i64 {
    ffi_guard(-1, move || {
        let p = CairoPattern {
            kind: CairoPatternKind::Solid(rgba(packed)),
            stops: Vec::new(),
        };
        let id = next_id();
        lock_recover(patterns()).insert(id, p);
        id
    })
}

/// Creates a linear-gradient pattern between two user-space points.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_create_linear(p0: i64, p1: i64) -> i64 {
    ffi_guard(-1, move || {
        let p = CairoPattern {
            kind: CairoPatternKind::Linear {
                p0: pt(p0),
                p1: pt(p1),
            },
            stops: Vec::new(),
        };
        let id = next_id();
        lock_recover(patterns()).insert(id, p);
        id
    })
}

/// Creates a radial-gradient pattern. tiny-skia uses a single end circle, so the
/// outer circle (`c1`, `r1`) is used; the inner radius is approximated as a stop.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_create_radial(_p_c0: i64, _r0_fx: i64, p_c1: i64, r1_fx: i64) -> i64 {
    ffi_guard(-1, move || {
        let p = CairoPattern {
            kind: CairoPatternKind::Radial {
                c1: pt(p_c1),
                r1: fx(r1_fx),
            },
            stops: Vec::new(),
        };
        let id = next_id();
        lock_recover(patterns()).insert(id, p);
        id
    })
}

/// Adds a color stop (offset in fixed-point milli 0..1000) to a gradient pattern.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_add_color_stop_rgba(pattern: i64, offset_fx: i64, packed: i64) {
    ffi_guard((), move || {
        if let Some(p) = lock_recover(patterns()).get_mut(&pattern) {
            let offset = (offset_fx as f32 / 1000.0).clamp(0.0, 1.0);
            p.stops.push((offset, color(packed)));
        }
    })
}

/// Destroys a pattern. Idempotent.
#[no_mangle]
pub extern "C" fn elephc_cairo_pattern_destroy(pattern: i64) {
    ffi_guard((), move || {
        lock_recover(patterns()).remove(&pattern);
    })
}

/// Sets the context's source to a pattern, building the shader in device space
/// using the current matrix. A solid pattern (or a gradient that fails to build)
/// falls back to a solid color.
#[no_mangle]
pub extern "C" fn elephc_cairo_set_source_pattern(ctx: i64, pattern: i64) {
    ffi_guard((), move || {
        let mut cguard = lock_recover(contexts());
        let Some(c) = cguard.get_mut(&ctx) else {
            return;
        };
        let pguard = lock_recover(patterns());
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
    })
}
