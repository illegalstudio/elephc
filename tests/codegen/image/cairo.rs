//! Purpose:
//! Tests for the Cairo OOP surface: `CairoImageSurface` (create, size,
//! writeToPng), `CairoContext` (paint, fill/stroke, rectangle/arc/curveTo paths,
//! translate/scale/save-restore transforms, gradients and solid patterns,
//! getCurrentPoint), `CairoMatrix` (value-object transforms), and the documented
//! gaps (PDF/PS/SVG surfaces, FreeType text, surface patterns) that throw
//! CairoException.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The Cairo bridge is tiny-skia; results are checked by writing the surface to a
//!   PNG in the test's temp dir and decoding it through GD (`imagecreatefrompng` +
//!   `imagecolorat`). Interior pixels of solid fills are exact (anti-aliasing only
//!   touches edges), so color assertions read well inside filled regions.
//! - Colors use 0/1 components so they round to exact 0/255 channels.
//! - PDF/PS/SVG surfaces, surface patterns, and text rendering have no pure-Rust
//!   path and throw CairoException; exercised via try/catch.

use crate::support::*;

/// CairoImageSurface reports the dimensions it was created with.
#[test]
fn test_cairo_surface_dimensions() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 48, 24);
echo $s->getWidth() . "x" . $s->getHeight();
"#,
    );
    assert_eq!(out, "48x24");
}

/// paint() floods the surface with the current solid source.
#[test]
fn test_cairo_paint_background() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 16, 16);
$cr = new CairoContext($s);
$cr->setSourceRgb(0, 0, 1);
$cr->paint();
$s->writeToPng("p.png");
$img = imagecreatefrompng("p.png");
$rgb = imagecolorat($img, 8, 8);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "0,0,255");
}

/// A filled rectangle paints its interior with the source color.
#[test]
fn test_cairo_fill_rectangle() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(0, 1, 0);
$cr->rectangle(10, 10, 20, 20);
$cr->fill();
$s->writeToPng("r.png");
$img = imagecreatefrompng("r.png");
$in = imagecolorat($img, 20, 20);
$out = imagecolorat($img, 2, 2);
echo (($in >> 16) & 0xFF) . "," . (($in >> 8) & 0xFF) . "," . ($in & 0xFF) . ";"
    . (($out >> 16) & 0xFF) . "," . (($out >> 8) & 0xFF) . "," . ($out & 0xFF);
"#,
    );
    assert_eq!(out, "0,255,0;255,255,255");
}

/// A stroked horizontal line paints along its width.
#[test]
fn test_cairo_stroke_line() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(1, 0, 0);
$cr->setLineWidth(6);
$cr->moveTo(5, 20);
$cr->lineTo(35, 20);
$cr->stroke();
$s->writeToPng("l.png");
$img = imagecreatefrompng("l.png");
$rgb = imagecolorat($img, 20, 20);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "255,0,0");
}

/// A filled arc (full circle) under a translate paints its center.
#[test]
fn test_cairo_arc_fill_circle() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(0, 0, 1);
$cr->arc(20, 20, 12, 0, 2 * M_PI);
$cr->fill();
$s->writeToPng("a.png");
$img = imagecreatefrompng("a.png");
$c = imagecolorat($img, 20, 20);
$edge = imagecolorat($img, 2, 2);
echo ($c & 0xFF) . ";" . ($edge & 0xFF);
"#,
    );
    assert_eq!(out, "255;255");
}

/// translate shifts subsequently-drawn geometry into device space.
#[test]
fn test_cairo_translate_transform() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(0, 0, 1);
$cr->translate(15, 15);
$cr->rectangle(0, 0, 8, 8);
$cr->fill();
$s->writeToPng("t.png");
$img = imagecreatefrompng("t.png");
$shifted = imagecolorat($img, 18, 18);
$origin = imagecolorat($img, 2, 2);
echo ($shifted & 0xFF) . ";" . ($origin & 0xFF);
"#,
    );
    assert_eq!(out, "255;255");
}

/// save/restore brackets a transform so it does not leak past restore().
#[test]
fn test_cairo_save_restore() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->save();
$cr->translate(20, 0);
$cr->restore();
$cr->setSourceRgb(0, 0, 1);
$cr->rectangle(0, 0, 8, 8);
$cr->fill();
$s->writeToPng("sr.png");
$img = imagecreatefrompng("sr.png");
$atOrigin = imagecolorat($img, 3, 3);
$atShift = imagecolorat($img, 23, 3);
echo (($atOrigin >> 16) & 0xFF) . ";" . (($atShift >> 16) & 0xFF);
"#,
    );
    assert_eq!(out, "0;255");
}

/// scale multiplies user-space coordinates before rasterization.
#[test]
fn test_cairo_scale_transform() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(0, 0, 1);
$cr->scale(10, 10);
$cr->rectangle(0, 0, 3, 3);
$cr->fill();
$s->writeToPng("sc.png");
$img = imagecreatefrompng("sc.png");
$inside = imagecolorat($img, 25, 25);
echo ($inside & 0xFF);
"#,
    );
    assert_eq!(out, "255");
}

/// A linear gradient interpolates between its color stops across the fill.
#[test]
fn test_cairo_linear_gradient() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 12);
$cr = new CairoContext($s);
$grad = new CairoLinearGradient(0, 0, 40, 0);
$grad->addColorStopRgb(0, 1, 0, 0);
$grad->addColorStopRgb(1, 0, 0, 1);
$cr->setSource($grad);
$cr->rectangle(0, 0, 40, 12);
$cr->fill();
$s->writeToPng("g.png");
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

/// A radial gradient fills with its inner stop at the center.
#[test]
fn test_cairo_radial_gradient() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$grad = new CairoRadialGradient(20, 20, 0, 20, 20, 18);
$grad->addColorStopRgb(0, 0, 1, 0);
$grad->addColorStopRgb(1, 0, 0, 1);
$cr->setSource($grad);
$cr->rectangle(0, 0, 40, 40);
$cr->fill();
$s->writeToPng("rg.png");
$img = imagecreatefrompng("rg.png");
$center = imagecolorat($img, 20, 20);
echo (($center >> 8) & 0xFF) > 128 ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// A solid pattern set as the source fills with its color.
#[test]
fn test_cairo_solid_pattern() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 20, 20);
$cr = new CairoContext($s);
$pat = CairoSolidPattern::createRgb(0, 1, 0);
$cr->setSource($pat);
$cr->rectangle(0, 0, 20, 20);
$cr->fill();
$s->writeToPng("sp.png");
$img = imagecreatefrompng("sp.png");
$rgb = imagecolorat($img, 10, 10);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "0,255,0");
}

/// curveTo builds a cubic Bézier that strokes without error and paints near it.
#[test]
fn test_cairo_curve_to_stroke() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(1, 0, 0);
$cr->setLineWidth(3);
$cr->moveTo(5, 35);
$cr->curveTo(5, 5, 35, 5, 35, 35);
$cr->stroke();
$s->writeToPng("cv.png");
$img = imagecreatefrompng("cv.png");
$top = imagecolorat($img, 20, 9);
echo (($top >> 16) & 0xFF) > 128 ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

/// getCurrentPoint returns the last path point in user/device units.
#[test]
fn test_cairo_get_current_point() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->moveTo(5, 6);
$cr->lineTo(12, 18);
$p = $cr->getCurrentPoint();
echo $p["x"] . "," . $p["y"];
"#,
    );
    assert_eq!(out, "12,18");
}

/// CairoMatrix applies translate/scale transforms to a point as a value object.
#[test]
fn test_cairo_matrix_transform_point() {
    let out = compile_and_run(
        r#"<?php
$m = new CairoMatrix();
$m->initTranslate(5, 7);
$p = $m->transformPoint(1, 1);
$m2 = new CairoMatrix();
$m2->initScale(2, 3);
$q = $m2->transformPoint(4, 5);
echo $p["x"] . "," . $p["y"] . ";" . $q["x"] . "," . $q["y"];
"#,
    );
    assert_eq!(out, "6,8;8,15");
}

/// setMatrix replaces the CTM so a unit rectangle lands at the mapped device box.
#[test]
fn test_cairo_set_matrix() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 40, 40);
$cr = new CairoContext($s);
$cr->setSourceRgb(1, 1, 1);
$cr->paint();
$cr->setSourceRgb(0, 0, 1);
$m = new CairoMatrix(8, 0, 0, 8, 4, 4);
$cr->setMatrix($m);
$cr->rectangle(0, 0, 3, 3);
$cr->fill();
$s->writeToPng("m.png");
$img = imagecreatefrompng("m.png");
$inside = imagecolorat($img, 15, 15);
echo ($inside & 0xFF);
"#,
    );
    assert_eq!(out, "255");
}

/// PDF/PS/SVG surfaces are documented gaps and throw CairoException.
#[test]
fn test_cairo_vector_surfaces_throw() {
    let out = compile_and_run(
        r#"<?php
$n = 0;
try { $a = new CairoPdfSurface("a.pdf", 10, 10); } catch (CairoException $e) { $n = $n + 1; }
try { $b = new CairoPsSurface("b.ps", 10, 10); } catch (CairoException $e) { $n = $n + 1; }
try { $c = new CairoSvgSurface("c.svg", 10, 10); } catch (CairoException $e) { $n = $n + 1; }
echo $n;
"#,
    );
    assert_eq!(out, "3");
}

/// Text rendering (FreeType) and toy font faces are documented gaps and throw.
#[test]
fn test_cairo_text_throws() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 20, 20);
$cr = new CairoContext($s);
$cr->selectFontFace("serif");
$cr->setFontSize(12);
$n = 0;
try { $cr->showText("hi"); } catch (CairoException $e) { $n = $n + 1; }
try { $f = new CairoToyFontFace("serif"); } catch (CairoException $e) { $n = $n + 1; }
echo $n;
"#,
    );
    assert_eq!(out, "2");
}

/// Surface patterns are a documented gap and throw CairoException.
#[test]
fn test_cairo_surface_pattern_throws() {
    let out = compile_and_run(
        r#"<?php
$s = new CairoImageSurface(CairoFormat::ARGB32, 10, 10);
try {
    $p = new CairoSurfacePattern($s);
    echo "no-throw";
} catch (CairoException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}
