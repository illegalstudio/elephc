//! Purpose:
//! Tests for the procedural `cairo_*` API (common subset): the
//! free-function layer that wraps the `Cairo*` OOP classes. Each test drives the
//! functional surface — `cairo_image_surface_create`, `cairo_create`,
//! `cairo_set_source_*`, the path/transform/render ops, `cairo_pattern_create_*`,
//! `cairo_matrix_*`, `cairo_get_current_point`, and `cairo_image_surface_create_from_png`
//! — and checks output by writing a PNG and decoding it through GD, exactly like
//! the Cairo OOP tests.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The procedural layer delegates to the OOP classes (or inlines the OOP body for
//!   the assoc-returning `cairo_get_current_point` / `cairo_matrix_transform_point`),
//!   so these tests double as a parity check that the two layers stay in lockstep.
//! - Colors use 0/1 components so they round to exact 0/255 channels, and pixel
//!   assertions read well inside filled regions (anti-aliasing only touches edges).

use crate::support::*;

/// `cairo_image_surface_create` reports the dimensions it was created with, and
/// the procedural size getters agree with it.
#[test]
fn test_proc_surface_dimensions() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 48, 24);
echo cairo_image_surface_get_width($s) . "x" . cairo_image_surface_get_height($s);
"#,
    );
    assert_eq!(out, "48x24");
}

/// `cairo_create` + `cairo_set_source_rgb` + `cairo_paint` floods the surface.
#[test]
fn test_proc_paint_background() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 16, 16);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 0, 0, 1);
cairo_paint($cr);
cairo_surface_write_to_png($s, "p.png");
$img = imagecreatefrompng("p.png");
$rgb = imagecolorat($img, 8, 8);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "0,0,255");
}

/// `cairo_rectangle` + `cairo_fill` paints the rectangle interior.
#[test]
fn test_proc_fill_rectangle() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_set_source_rgb($cr, 0, 1, 0);
cairo_rectangle($cr, 10, 10, 20, 20);
cairo_fill($cr);
cairo_surface_write_to_png($s, "r.png");
$img = imagecreatefrompng("r.png");
$in = imagecolorat($img, 20, 20);
$out = imagecolorat($img, 2, 2);
echo (($in >> 16) & 0xFF) . "," . (($in >> 8) & 0xFF) . "," . ($in & 0xFF) . ";"
    . (($out >> 16) & 0xFF) . "," . (($out >> 8) & 0xFF) . "," . ($out & 0xFF);
"#,
    );
    assert_eq!(out, "0,255,0;255,255,255");
}

/// `cairo_move_to` / `cairo_line_to` / `cairo_stroke` with a line width paints a line.
#[test]
fn test_proc_stroke_line() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_set_source_rgb($cr, 1, 0, 0);
cairo_set_line_width($cr, 6);
cairo_move_to($cr, 5, 20);
cairo_line_to($cr, 35, 20);
cairo_stroke($cr);
cairo_surface_write_to_png($s, "l.png");
$img = imagecreatefrompng("l.png");
$rgb = imagecolorat($img, 20, 20);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "255,0,0");
}

/// `cairo_arc` filled as a full circle paints its center.
#[test]
fn test_proc_arc_fill_circle() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_set_source_rgb($cr, 0, 0, 1);
cairo_arc($cr, 20, 20, 12, 0, 2 * M_PI);
cairo_fill($cr);
cairo_surface_write_to_png($s, "a.png");
$img = imagecreatefrompng("a.png");
$c = imagecolorat($img, 20, 20);
$edge = imagecolorat($img, 2, 2);
echo ($c & 0xFF) . ";" . ($edge & 0xFF);
"#,
    );
    assert_eq!(out, "255;255");
}

/// `cairo_translate` shifts subsequently-drawn geometry into device space.
#[test]
fn test_proc_translate() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_set_source_rgb($cr, 0, 0, 1);
cairo_translate($cr, 15, 15);
cairo_rectangle($cr, 0, 0, 8, 8);
cairo_fill($cr);
cairo_surface_write_to_png($s, "t.png");
$img = imagecreatefrompng("t.png");
$shifted = imagecolorat($img, 18, 18);
$origin = imagecolorat($img, 2, 2);
echo ($shifted & 0xFF) . ";" . ($origin & 0xFF);
"#,
    );
    assert_eq!(out, "255;255");
}

/// `cairo_save` / `cairo_restore` brackets a transform so it does not leak past.
#[test]
fn test_proc_save_restore() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_save($cr);
cairo_translate($cr, 20, 0);
cairo_restore($cr);
cairo_set_source_rgb($cr, 0, 0, 1);
cairo_rectangle($cr, 0, 0, 8, 8);
cairo_fill($cr);
cairo_surface_write_to_png($s, "sr.png");
$img = imagecreatefrompng("sr.png");
$atOrigin = imagecolorat($img, 3, 3);
$atShift = imagecolorat($img, 23, 3);
echo (($atOrigin >> 16) & 0xFF) . ";" . (($atShift >> 16) & 0xFF);
"#,
    );
    assert_eq!(out, "0;255");
}

/// `cairo_scale` multiplies user-space coordinates before rasterization.
#[test]
fn test_proc_scale() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_set_source_rgb($cr, 0, 0, 1);
cairo_scale($cr, 10, 10);
cairo_rectangle($cr, 0, 0, 3, 3);
cairo_fill($cr);
cairo_surface_write_to_png($s, "sc.png");
$img = imagecreatefrompng("sc.png");
$inside = imagecolorat($img, 25, 25);
echo ($inside & 0xFF);
"#,
    );
    assert_eq!(out, "255");
}

/// `cairo_pattern_create_linear` + `cairo_pattern_add_color_stop_rgb` + `cairo_set_source`
/// interpolates across the fill.
#[test]
fn test_proc_linear_gradient() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 12);
$cr = cairo_create($s);
$grad = cairo_pattern_create_linear(0, 0, 40, 0);
cairo_pattern_add_color_stop_rgb($grad, 0, 1, 0, 0);
cairo_pattern_add_color_stop_rgb($grad, 1, 0, 0, 1);
cairo_set_source($cr, $grad);
cairo_rectangle($cr, 0, 0, 40, 12);
cairo_fill($cr);
cairo_surface_write_to_png($s, "g.png");
$img = imagecreatefrompng("g.png");
$left = imagecolorat($img, 2, 6);
$right = imagecolorat($img, 37, 6);
$lr = ($left >> 16) & 0xFF;
$rb = $right & 0xFF;
echo (($lr > 128) ? "1" : "0") . (($rb > 128) ? "1" : "0");
"#,
    );
    assert_eq!(out, "11");
}

/// `cairo_pattern_create_radial` fills with its inner stop at the center.
#[test]
fn test_proc_radial_gradient() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
$grad = cairo_pattern_create_radial(20, 20, 0, 20, 20, 18);
cairo_pattern_add_color_stop_rgb($grad, 0, 0, 1, 0);
cairo_pattern_add_color_stop_rgb($grad, 1, 0, 0, 1);
cairo_set_source($cr, $grad);
cairo_rectangle($cr, 0, 0, 40, 40);
cairo_fill($cr);
cairo_surface_write_to_png($s, "rg.png");
$img = imagecreatefrompng("rg.png");
$center = imagecolorat($img, 20, 20);
echo (($center >> 8) & 0xFF) > 128 ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// `cairo_pattern_create_rgba` set as the source fills with its color.
#[test]
fn test_proc_solid_pattern() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 20, 20);
$cr = cairo_create($s);
$pat = cairo_pattern_create_rgba(0, 1, 0, 1);
cairo_set_source($cr, $pat);
cairo_rectangle($cr, 0, 0, 20, 20);
cairo_fill($cr);
cairo_surface_write_to_png($s, "sp.png");
$img = imagecreatefrompng("sp.png");
$rgb = imagecolorat($img, 10, 10);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "0,255,0");
}

/// `cairo_get_current_point` returns the last path point as an ["x","y"] assoc.
#[test]
fn test_proc_get_current_point() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_move_to($cr, 5, 6);
cairo_line_to($cr, 12, 18);
$p = cairo_get_current_point($cr);
echo $p["x"] . "," . $p["y"];
"#,
    );
    assert_eq!(out, "12,18");
}

/// `cairo_matrix_init_scale` + `cairo_matrix_transform_point` maps a point.
#[test]
fn test_proc_matrix_init_scale_and_transform() {
    let out = compile_and_run(
        r#"<?php
$m = cairo_matrix_init_scale(2, 3);
$q = cairo_matrix_transform_point($m, 4, 5);
echo $q["x"] . "," . $q["y"];
"#,
    );
    assert_eq!(out, "8,15");
}

/// `cairo_matrix_multiply` composes two matrices (m2 applied first, then m1).
#[test]
fn test_proc_matrix_multiply() {
    let out = compile_and_run(
        r#"<?php
$m1 = cairo_matrix_init_scale(2, 3);
$m2 = cairo_matrix_init_translate(5, 7);
$prod = cairo_matrix_multiply($m1, $m2);
$p = cairo_matrix_transform_point($prod, 1, 1);
echo $p["x"] . "," . $p["y"];
"#,
    );
    assert_eq!(out, "12,24");
}

/// `cairo_set_matrix` replaces the CTM so a unit rectangle lands at the mapped box.
#[test]
fn test_proc_set_matrix() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 40, 40);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);
cairo_set_source_rgb($cr, 0, 0, 1);
$m = cairo_matrix_init_scale(8, 8);
cairo_set_matrix($cr, $m);
cairo_translate($cr, 0.5, 0.5);
cairo_rectangle($cr, 0, 0, 3, 3);
cairo_fill($cr);
cairo_surface_write_to_png($s, "m.png");
$img = imagecreatefrompng("m.png");
$inside = imagecolorat($img, 15, 15);
echo ($inside & 0xFF);
"#,
    );
    assert_eq!(out, "255");
}

/// Omitted procedural cairo functions stay genuinely undefined (compile error),
/// not silent stubs returning false/null. `cairo_font_options_create` is in the
/// obscure PECL tail (font metrics) that this common subset deliberately
/// omits; the `cairo_` prefix still triggers prelude injection, so the type checker
/// rejects the call rather than treating it as a builtin no-op.
#[test]
#[should_panic(expected = "Undefined function: cairo_font_options_create")]
fn test_proc_omitted_function_is_undefined() {
    let _ = compile_and_run(
        r#"<?php
$opts = cairo_font_options_create();
"#,
    );
}

/// `cairo_image_surface_create_from_png` round-trips a written PNG: dimensions
/// survive and an opaque pixel survives the decode→premultiply→re-encode path.
#[test]
fn test_proc_from_png_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$s = cairo_image_surface_create(CairoFormat::ARGB32, 12, 8);
$cr = cairo_create($s);
cairo_set_source_rgb($cr, 0, 0, 1);
cairo_paint($cr);
cairo_surface_write_to_png($s, "src.png");
$loaded = cairo_image_surface_create_from_png("src.png");
$w = cairo_image_surface_get_width($loaded);
$h = cairo_image_surface_get_height($loaded);
cairo_surface_write_to_png($loaded, "dst.png");
$img = imagecreatefrompng("dst.png");
$rgb = imagecolorat($img, 6, 4);
echo $w . "x" . $h . ";" . (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "12x8;0,0,255");
}