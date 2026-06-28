//! Purpose:
//! Tests for the Gmagick OOP surface: the `Gmagick` wand (newImage,
//! read/write/blob, geometry, fluent resize/scale/crop/rotate/flip/flop, blur,
//! modulate, compositing, multi-frame navigation), `GmagickDraw` (fill/stroke plus
//! line/rectangle/ellipse/point/polygon), and `GmagickPixel` (color parsing and
//! channel get/set). Also covers the documented unsupported-operator gaps.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Gmagick has no per-pixel read method (matching PHP), so drawing/compositing
//!   results are checked by encoding the frame with `getImageBlob()` and decoding
//!   it through GD (`imagecreatefromstring` + `imagecolorat`) — a cross-bridge
//!   round-trip that does not depend on raw encoder bytes.
//! - Gmagick methods are fluent (`return $this`); chained calls exercise the
//!   receiver-acquire/release ownership path in addition to the operation itself.
//! - Unsupported operators (`COMPOSITE_MULTIPLY`, `swirlImage`, …) and invalid
//!   colors throw `GmagickException`/`GmagickPixelException`, exercised via
//!   try/catch since they are runtime throws. The pure-Rust bridge needs no system
//!   GraphicsMagick, so these fixtures are not `#[ignore]`d.

use crate::support::*;

/// `newImage` sets the frame dimensions, readable via getImageWidth/Height and the
/// string-keyed getImageGeometry array.
#[test]
fn test_gmagick_new_image_geometry() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(20, 10, "white");
$g = $gm->getImageGeometry();
echo $gm->getImageWidth() . "x" . $gm->getImageHeight() . ";" . $g["width"] . "," . $g["height"];
"#,
    );
    assert_eq!(out, "20x10;20,10");
}

/// `GmagickPixel` parses named, hex, and rgb() colors identically; getColor exposes
/// the channels by name and getColorAsString is the canonical srgb form.
#[test]
fn test_gmagick_pixel_parsing() {
    let out = compile_and_run(
        r##"<?php
$a = new GmagickPixel("red");
$b = new GmagickPixel("#ff0000");
$c = new GmagickPixel("rgb(255,0,0)");
$ca = $a->getColor();
echo $a->getColorAsString() . ";" . ($a->getColorAsString() === $b->getColorAsString() ? "1" : "0")
    . ($a->getColorAsString() === $c->getColorAsString() ? "1" : "0") . ";"
    . $ca["r"] . "," . $ca["g"] . "," . $ca["b"];
"##,
    );
    assert_eq!(out, "srgb(255,0,0);11;255,0,0");
}

/// `getColorValue` returns each channel as a 0..1 float by Gmagick::COLOR_* code.
#[test]
fn test_gmagick_pixel_color_value() {
    let out = compile_and_run(
        r#"<?php
$p = new GmagickPixel("rgb(255,0,0)");
echo $p->getColorValue(Gmagick::COLOR_RED) . ";" . $p->getColorValue(Gmagick::COLOR_GREEN);
"#,
    );
    assert_eq!(out, "1;0");
}

/// `setColorValue` mutates one channel from a 0..1 float and returns the pixel.
#[test]
fn test_gmagick_pixel_set_color_value() {
    let out = compile_and_run(
        r#"<?php
$p = new GmagickPixel("black");
$p->setColorValue(Gmagick::COLOR_GREEN, 1.0);
$c = $p->getColor();
echo $c["r"] . "," . $c["g"] . "," . $c["b"];
"#,
    );
    assert_eq!(out, "0,255,0");
}

/// `GmagickDraw` fills a rectangle; the result is verified by decoding the frame's
/// PNG blob through GD and reading the painted pixel.
#[test]
fn test_gmagick_draw_rectangle_readback() {
    let out = compile_and_run(
        r##"<?php
$gm = new Gmagick();
$gm->newImage(20, 20, "white");
$draw = new GmagickDraw();
$draw->setFillColor("#1d4ed8")->rectangle(4, 4, 14, 14);
$gm->drawImage($draw);
$img = imagecreatefromstring($gm->getImageBlob());
$rgb = imagecolorat($img, 9, 9);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"##,
    );
    assert_eq!(out, "29,78,216");
}

/// `GmagickDraw` strokes a polygon; an interior fill pixel is verified via GD.
#[test]
fn test_gmagick_draw_polygon_readback() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(20, 20, "white");
$draw = new GmagickDraw();
$draw->setFillColor("rgb(0,128,0)");
$draw->polygon([["x" => 2, "y" => 2], ["x" => 17, "y" => 2], ["x" => 17, "y" => 17], ["x" => 2, "y" => 17]]);
$gm->drawImage($draw);
$img = imagecreatefromstring($gm->getImageBlob());
$rgb = imagecolorat($img, 9, 9);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "0,128,0");
}

/// Fluent chaining of format + scale returns the Gmagick object each step and
/// applies both operations (exercises the `return $this` ownership path).
#[test]
fn test_gmagick_fluent_format_and_scale() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(10, 10, "white");
$gm->setImageFormat("PNG")->scaleImage(40, 20);
echo $gm->getImageWidth() . "x" . $gm->getImageHeight() . ";" . $gm->getImageFormat();
"#,
    );
    assert_eq!(out, "40x20;PNG");
}

/// resize/crop change the frame dimensions as requested.
#[test]
fn test_gmagick_resize_and_crop() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(40, 30, "white");
$gm->resizeImage(20, 15, Gmagick::FILTER_LANCZOS, 1.0);
$a = $gm->getImageWidth() . "x" . $gm->getImageHeight();
$gm->cropImage(8, 6, 2, 2);
echo $a . ";" . $gm->getImageWidth() . "x" . $gm->getImageHeight();
"#,
    );
    assert_eq!(out, "20x15;8x6");
}

/// rotateImage by 90° swaps width/height; an int angle reaches the float parameter.
#[test]
fn test_gmagick_rotate_int_degrees() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(10, 4, "white");
$gm->rotateImage("black", 90);
echo $gm->getImageWidth() . "x" . $gm->getImageHeight();
"#,
    );
    assert_eq!(out, "4x10");
}

/// flip/flop keep the dimensions and return the object for chaining.
#[test]
fn test_gmagick_flip_flop() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(12, 8, "white");
$gm->flipImage()->flopImage();
echo $gm->getImageWidth() . "x" . $gm->getImageHeight();
"#,
    );
    assert_eq!(out, "12x8");
}

/// addImage appends frames; getNumberImages/getImageIndex/nextImage navigate them.
#[test]
fn test_gmagick_multiframe_navigation() {
    let out = compile_and_run(
        r#"<?php
$a = new Gmagick();
$a->newImage(5, 5, "red");
$b = new Gmagick();
$b->newImage(6, 6, "blue");
$a->addImage($b);
$n = $a->getNumberImages();
$a->setImageIndex(0);
$has = $a->hasNextImage() ? "1" : "0";
$a->nextImage();
echo $n . ";" . $a->getImageIndex() . ";" . $has . ";" . $a->getImageWidth();
"#,
    );
    assert_eq!(out, "2;1;1;6");
}

/// getImageBlob → readImageBlob round-trips a frame, preserving its dimensions.
#[test]
fn test_gmagick_blob_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(24, 18, "rgb(10,20,30)");
$blob = $gm->getImageBlob();
$gm2 = new Gmagick();
$gm2->readImageBlob($blob);
echo $gm2->getImageWidth() . "x" . $gm2->getImageHeight();
"#,
    );
    assert_eq!(out, "24x18");
}

/// writeImage → readImage round-trips through a PNG file in the temp dir.
#[test]
fn test_gmagick_file_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(16, 9, "white");
$gm->writeImage("gm_out.png");
$gm2 = new Gmagick();
$gm2->readImage("gm_out.png");
echo $gm2->getImageWidth() . "x" . $gm2->getImageHeight();
"#,
    );
    assert_eq!(out, "16x9");
}

/// compositeImage with COMPOSITE_OVER blends the source frame at an offset; the
/// composited pixel is verified through GD.
#[test]
fn test_gmagick_composite_over_readback() {
    let out = compile_and_run(
        r#"<?php
$base = new Gmagick();
$base->newImage(20, 20, "white");
$top = new Gmagick();
$top->newImage(8, 8, "rgb(200,40,40)");
$base->compositeImage($top, Gmagick::COMPOSITE_OVER, 5, 5);
$img = imagecreatefromstring($base->getImageBlob());
$rgb = imagecolorat($img, 8, 8);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "200,40,40");
}

/// An unsupported composite operator throws GmagickException.
#[test]
fn test_gmagick_unsupported_composite_throws() {
    let out = compile_and_run(
        r#"<?php
$base = new Gmagick();
$base->newImage(10, 10, "white");
$top = new Gmagick();
$top->newImage(4, 4, "red");
try {
    $base->compositeImage($top, Gmagick::COMPOSITE_MULTIPLY, 0, 0);
    echo "no-throw";
} catch (GmagickException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// Documented unsupported effects throw GmagickException.
#[test]
fn test_gmagick_unsupported_effects_throw() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(10, 10, "white");
$n = 0;
try { $gm->swirlImage(90.0); } catch (GmagickException $e) { $n = $n + 1; }
try { $gm->charcoalImage(1.0, 0.5); } catch (GmagickException $e) { $n = $n + 1; }
try { $gm->oilPaintImage(2.0); } catch (GmagickException $e) { $n = $n + 1; }
echo $n;
"#,
    );
    assert_eq!(out, "3");
}

/// GmagickDraw::annotate (FreeType text) throws GmagickDrawException.
#[test]
fn test_gmagick_draw_annotate_throws() {
    let out = compile_and_run(
        r#"<?php
$draw = new GmagickDraw();
try {
    $draw->annotate(1.0, 2.0, "hi");
    echo "no-throw";
} catch (GmagickDrawException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// An unrecognized color throws GmagickPixelException.
#[test]
fn test_gmagick_bad_color_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $p = new GmagickPixel("notacolor");
    echo "no-throw";
} catch (GmagickPixelException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// setImageFormat rejects unsupported formats and queryFormats lists the codecs.
#[test]
fn test_gmagick_format_and_queryformats() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(4, 4, "white");
$bad = 0;
try { $gm->setImageFormat("TIFF"); } catch (GmagickException $e) { $bad = 1; }
$fmts = $gm->queryFormats();
echo $bad . ";" . count($fmts) . ";" . (in_array("PNG", $fmts) ? "1" : "0");
"#,
    );
    assert_eq!(out, "1;5;1");
}

/// setImageBackgroundColor paints the current frame; verified through GD.
#[test]
fn test_gmagick_set_background_color() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(8, 8, "white");
$gm->setImageBackgroundColor("rgb(12,34,56)");
$img = imagecreatefromstring($gm->getImageBlob());
$rgb = imagecolorat($img, 4, 4);
echo (($rgb >> 16) & 0xFF) . "," . (($rgb >> 8) & 0xFF) . "," . ($rgb & 0xFF);
"#,
    );
    assert_eq!(out, "12,34,56");
}

/// Compression quality round-trips through the setter/getter.
#[test]
fn test_gmagick_compression_quality() {
    let out = compile_and_run(
        r#"<?php
$gm = new Gmagick();
$gm->newImage(4, 4, "white");
$gm->setCompressionQuality(73);
echo $gm->getCompressionQuality();
"#,
    );
    assert_eq!(out, "73");
}
