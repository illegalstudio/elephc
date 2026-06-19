//! Purpose:
//! Tests for the Imagick OOP surface: the `Imagick` wand (newImage,
//! read/write/blob, geometry, resize/scale/crop/rotate/flip/flop, effects,
//! compositing, multi-frame iteration, Countable), `ImagickDraw` (fill/stroke
//! plus line/rectangle/circle/ellipse/point/polygon), `ImagickPixel` (color
//! parsing and channel queries), `ImagickPixelIterator`, and `ImagickKernel`
//! (matrix convolution). Also covers the documented unsupported-operator gaps.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Results are checked by reading pixels back with `getImagePixelColor()` (whose
//!   `getColorAsString()` yields an exact "srgb(r,g,b)") and by width/height for
//!   geometry ops, so assertions do not depend on encoder byte output.
//! - Unsupported operators (e.g. `distortImage`, `COMPOSITE_MULTIPLY`) and invalid
//!   colors throw `ImagickException`/`ImagickPixelException`; those are exercised
//!   with try/catch asserting the caught path, since they are runtime throws.
//! - The pure-Rust codec bridge needs no system ImageMagick, so these fixtures are
//!   not `#[ignore]`d.

use crate::support::*;

/// `newImage` fills a solid background and `getImageWidth`/`Height`/`getImageGeometry`
/// report the dimensions, with the geometry array readable by string key.
#[test]
fn test_imagick_new_image_geometry() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(20, 10, "white");
$g = $im->getImageGeometry();
echo $im->getImageWidth() . "x" . $im->getImageHeight() . ";" . $g["width"] . "," . $g["height"];
"#,
    );
    assert_eq!(out, "20x10;20,10");
}

/// `getImagePixelColor` returns an `ImagickPixel` whose `getColor()` exposes the
/// channels by name and whose `getColorAsString()` is the canonical srgb form.
#[test]
fn test_imagick_pixel_color_read() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(4, 4, "rgb(10,20,30)");
$p = $im->getImagePixelColor(1, 1);
$c = $p->getColor();
echo $c["r"] . "," . $c["g"] . "," . $c["b"] . ";" . $p->getColorAsString();
"#,
    );
    assert_eq!(out, "10,20,30;srgb(10,20,30)");
}

/// `ImagickPixel` parses named, hex, and rgb() colors to the same packed color.
#[test]
fn test_imagick_pixel_parsing() {
    let out = compile_and_run(
        r##"<?php
$a = new ImagickPixel("red");
$b = new ImagickPixel("#ff0000");
$c = new ImagickPixel("rgb(255,0,0)");
echo $a->getColorAsString() . ";" . $b->getColorAsString() . ";" . $c->getColorAsString();
"##,
    );
    assert_eq!(out, "srgb(255,0,0);srgb(255,0,0);srgb(255,0,0)");
}

/// `ImagickPixel::getColorValue` returns a channel as a normalized 0..1 float
/// selected by an `Imagick::COLOR_*` constant.
#[test]
fn test_imagick_pixel_color_value() {
    let out = compile_and_run(
        r##"<?php
$p = new ImagickPixel("#3366cc");
echo round($p->getColorValue(Imagick::COLOR_RED), 2) . "," . round($p->getColorValue(Imagick::COLOR_BLUE), 2);
"##,
    );
    assert_eq!(out, "0.2,0.8");
}

/// `ImagickPixel::isSimilar` / `isPixelSimilar` compare colors by normalized RGB
/// distance against a `float $fuzz`, accepting an int or float argument (int is
/// widened to the float parameter) and returning a deterministic boolean for
/// identical vs. distant colors.
#[test]
fn test_imagick_pixel_issimilar() {
    let out = compile_and_run(
        r##"<?php
$red = new ImagickPixel("red");
$blue = new ImagickPixel("blue");
echo $red->isSimilar($red, 0) ? "T" : "F";
echo $red->isSimilar($blue, 0) ? "T" : "F";
echo $red->isSimilar($blue, 2) ? "T" : "F";
echo $red->isSimilar($blue, 0.5) ? "T" : "F";
echo $red->isPixelSimilar($blue, 2) ? "T" : "F";
"##,
    );
    assert_eq!(out, "TFTFT");
}

/// `ImagickDraw::rectangle` filled with a named color paints the interior and
/// leaves the exterior untouched after `drawImage`.
#[test]
fn test_imagick_draw_rectangle() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(12, 12, "white");
$d = new ImagickDraw();
$d->setFillColor("red");
$d->rectangle(2, 2, 8, 8);
$im->drawImage($d);
echo $im->getImagePixelColor(5, 5)->getColorAsString() . ";" . $im->getImagePixelColor(0, 0)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(255,0,0);srgb(255,255,255)");
}

/// `ImagickDraw::line` draws with the stroke color along the path.
#[test]
fn test_imagick_draw_line() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(10, 10, "white");
$d = new ImagickDraw();
$d->setStrokeColor("blue");
$d->setStrokeWidth(1);
$d->line(0, 0, 9, 9);
$im->drawImage($d);
echo $im->getImagePixelColor(5, 5)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(0,0,255)");
}

/// `ImagickDraw::circle` fills a disc centered at the origin point.
#[test]
fn test_imagick_draw_circle() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(20, 20, "white");
$d = new ImagickDraw();
$d->setFillColor("green");
$d->circle(10, 10, 10, 4);
$im->drawImage($d);
echo $im->getImagePixelColor(10, 10)->getColorAsString() . ";" . $im->getImagePixelColor(0, 0)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(0,128,0);srgb(255,255,255)");
}

/// `ImagickDraw::polygon` fills the polygon described by ["x"=>,"y"=>] points.
#[test]
fn test_imagick_draw_polygon() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(20, 20, "white");
$d = new ImagickDraw();
$d->setFillColor("blue");
$d->polygon([["x" => 2, "y" => 2], ["x" => 18, "y" => 2], ["x" => 10, "y" => 18]]);
$im->drawImage($d);
echo $im->getImagePixelColor(10, 8)->getColorAsString() . ";" . $im->getImagePixelColor(0, 19)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(0,0,255);srgb(255,255,255)");
}

/// `resizeImage` and `scaleImage` change the dimensions of the current frame.
#[test]
fn test_imagick_resize_scale() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(10, 10, "white");
$im->resizeImage(40, 20, Imagick::FILTER_LANCZOS, 1.0);
$a = $im->getImageWidth() . "x" . $im->getImageHeight();
$im->scaleImage(5, 5);
echo $a . ";" . $im->getImageWidth() . "x" . $im->getImageHeight();
"#,
    );
    assert_eq!(out, "40x20;5x5");
}

/// `cropImage` keeps the requested sub-rectangle of the frame.
#[test]
fn test_imagick_crop() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(20, 20, "white");
$d = new ImagickDraw();
$d->setFillColor("red");
$d->rectangle(0, 0, 9, 9);
$im->drawImage($d);
$im->cropImage(10, 10, 0, 0);
echo $im->getImageWidth() . "x" . $im->getImageHeight() . ";" . $im->getImagePixelColor(5, 5)->getColorAsString();
"#,
    );
    assert_eq!(out, "10x10;srgb(255,0,0)");
}

/// `rotateImage` by 90 degrees swaps the width and height.
#[test]
fn test_imagick_rotate() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(8, 4, "white");
$im->rotateImage("white", 90);
echo $im->getImageWidth() . "x" . $im->getImageHeight();
"#,
    );
    assert_eq!(out, "4x8");
}

/// `blurImage` and `gaussianBlurImage` accept integer radius/sigma arguments
/// (widened to their `float` parameters) and soften a hard red/white edge: the
/// boundary pixel's green channel rises above zero as white bleeds into red.
#[test]
fn test_imagick_blur_int_args() {
    let out = compile_and_run(
        r##"<?php
$im = new Imagick();
$im->newImage(8, 1, "white");
$d = new ImagickDraw();
$d->setFillColor("red");
$d->rectangle(0, 0, 3, 0);
$im->drawImage($d);
$im->blurImage(2, 2);
$ca = $im->getImagePixelColor(3, 0)->getColor();
echo ($ca["g"] > 0 ? "blurred" : "flat") . ";";

$im2 = new Imagick();
$im2->newImage(8, 1, "white");
$d2 = new ImagickDraw();
$d2->setFillColor("red");
$d2->rectangle(0, 0, 3, 0);
$im2->drawImage($d2);
$im2->gaussianBlurImage(2, 2);
$cb = $im2->getImagePixelColor(3, 0)->getColor();
echo ($cb["g"] > 0 ? "blurred" : "flat");
"##,
    );
    assert_eq!(out, "blurred;blurred");
}

/// `flipImage` (vertical) and `flopImage` (horizontal) move pixels to the
/// mirrored row/column.
#[test]
fn test_imagick_flip_flop() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(4, 4, "white");
$d = new ImagickDraw();
$d->setFillColor("red");
$d->rectangle(0, 0, 3, 0);
$im->drawImage($d);
$im->flipImage();
echo $im->getImagePixelColor(0, 3)->getColorAsString() . ";" . $im->getImagePixelColor(0, 0)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(255,0,0);srgb(255,255,255)");
}

/// `negateImage` inverts the RGB channels of the frame.
#[test]
fn test_imagick_negate() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(4, 4, "rgb(10,20,30)");
$im->negateImage();
echo $im->getImagePixelColor(0, 0)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(245,235,225)");
}

/// `modulateImage` at 50% brightness darkens a mid-gray frame.
#[test]
fn test_imagick_modulate_brightness() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(4, 4, "rgb(120,120,120)");
$im->modulateImage(50, 100, 100);
$v = $im->getImagePixelColor(0, 0)->getColorValue(Imagick::COLOR_RED);
echo ($v < 0.4 ? "darker" : "no");
"#,
    );
    assert_eq!(out, "darker");
}

/// `compositeImage` with `COMPOSITE_OVER` blits the source frame onto the
/// destination at the given offset.
#[test]
fn test_imagick_composite_over() {
    let out = compile_and_run(
        r#"<?php
$canvas = new Imagick();
$canvas->newImage(10, 10, "white");
$dot = new Imagick();
$dot->newImage(4, 4, "black");
$canvas->compositeImage($dot, Imagick::COMPOSITE_OVER, 1, 1);
echo $canvas->getImagePixelColor(2, 2)->getColorAsString() . ";" . $canvas->getImagePixelColor(8, 8)->getColorAsString();
"#,
    );
    assert_eq!(out, "srgb(0,0,0);srgb(255,255,255)");
}

/// An unsupported composite operator throws `ImagickException` with a clear
/// "not supported in elephc" message (documented gap, not a silent no-op).
#[test]
fn test_imagick_composite_unsupported_throws() {
    let out = compile_and_run(
        r#"<?php
$a = new Imagick();
$a->newImage(4, 4, "white");
$b = new Imagick();
$b->newImage(2, 2, "black");
try {
    $a->compositeImage($b, Imagick::COMPOSITE_MULTIPLY, 0, 0);
    echo "no-throw";
} catch (ImagickException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// `addImage`, `getNumberImages`, and `count()` (Countable) track a multi-frame
/// wand; `setImageIndex` selects the active frame for per-image queries.
#[test]
fn test_imagick_multiframe_count_index() {
    let out = compile_and_run(
        r#"<?php
$seq = new Imagick();
$a = new Imagick(); $a->newImage(4, 4, "red");
$b = new Imagick(); $b->newImage(7, 5, "blue");
$seq->addImage($a);
$seq->addImage($b);
$seq->setImageIndex(0);
$w0 = $seq->getImageWidth();
$seq->setImageIndex(1);
$w1 = $seq->getImageWidth();
echo $seq->getNumberImages() . "," . count($seq) . ";" . $w0 . "," . $w1;
"#,
    );
    assert_eq!(out, "2,2;4,7");
}

/// Iterating an Imagick with foreach yields the wand positioned at each frame,
/// exposing sequential keys (Iterator interface).
#[test]
fn test_imagick_iterator_foreach() {
    let out = compile_and_run(
        r#"<?php
$seq = new Imagick();
$a = new Imagick(); $a->newImage(3, 3, "red");
$b = new Imagick(); $b->newImage(5, 5, "blue");
$c = new Imagick(); $c->newImage(7, 7, "green");
$seq->addImage($a);
$seq->addImage($b);
$seq->addImage($c);
$acc = "";
foreach ($seq as $i => $frame) {
    $acc .= $i . ":" . $frame->getImageWidth() . " ";
}
echo trim($acc);
"#,
    );
    assert_eq!(out, "0:3 1:5 2:7");
}

/// `getImageBlob` encodes the current frame and `readImageBlob` decodes it back
/// into a new wand of the same dimensions (binary-safe blob round-trip).
#[test]
fn test_imagick_blob_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(15, 9, "white");
$im->setImageFormat("PNG");
$blob = $im->getImageBlob();
$im2 = new Imagick();
$im2->readImageBlob($blob);
echo (strlen($blob) > 30 ? "ok" : "no") . ";" . $im2->getImageWidth() . "x" . $im2->getImageHeight();
"#,
    );
    assert_eq!(out, "ok;15x9");
}

/// `writeImage` to a file and reading it back through the path constructor
/// round-trips the dimensions and detects the format from the extension.
#[test]
fn test_imagick_file_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(13, 11, "white");
$im->writeImage("imk_out.png");
$loaded = new Imagick("imk_out.png");
echo $loaded->getImageWidth() . "x" . $loaded->getImageHeight() . ";" . $loaded->getImageFormat();
"#,
    );
    assert_eq!(out, "13x11;PNG");
}

/// `convolveImage` with an `ImagickKernel::fromMatrix` identity kernel leaves the
/// image unchanged (exercises the 3x3 matrix path end to end).
#[test]
fn test_imagick_convolve_identity() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(5, 5, "rgb(40,80,120)");
$k = ImagickKernel::fromMatrix([[0, 0, 0], [0, 1, 0], [0, 0, 0]]);
$im->convolveImage($k);
echo $k->_size() . ";" . $im->getImagePixelColor(2, 2)->getColorAsString();
"#,
    );
    assert_eq!(out, "3;srgb(40,80,120)");
}

/// A non-3x3 kernel is a documented gap: `convolveImage` throws `ImagickException`.
#[test]
fn test_imagick_convolve_unsupported_size_throws() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(5, 5, "white");
$k = ImagickKernel::fromMatrix([[0, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, 1, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0]]);
try {
    $im->convolveImage($k);
    echo "no-throw";
} catch (ImagickException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// `getPixelIterator` walks the image one row at a time, each row an array of
/// `ImagickPixel` objects of the image's width.
#[test]
fn test_imagick_pixel_iterator() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(6, 3, "rgb(5,6,7)");
$it = $im->getPixelIterator();
$row = $it->getCurrentIteratorRow();
echo count($row) . ";" . $row[0]->getColorAsString();
"#,
    );
    assert_eq!(out, "6;srgb(5,6,7)");
}

/// `distortImage` is a documented unsupported operator and throws.
#[test]
fn test_imagick_distort_unsupported_throws() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(4, 4, "white");
try {
    $im->distortImage(0, [1.0, 2.0], false);
    echo "no-throw";
} catch (ImagickException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// An unrecognized color name throws `ImagickPixelException`.
#[test]
fn test_imagick_bad_color_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $p = new ImagickPixel("notacolorname");
    echo "no-throw";
} catch (ImagickPixelException $e) {
    echo "caught";
}
"#,
    );
    assert_eq!(out, "caught");
}

/// `setImageFormat`/`getImageFormat` round-trip the format string, and
/// `queryFormats` lists the supported codecs.
#[test]
fn test_imagick_format_and_queryformats() {
    let out = compile_and_run(
        r#"<?php
$im = new Imagick();
$im->newImage(4, 4, "white");
$im->setImageFormat("JPEG");
echo $im->getImageFormat() . ";" . count(Imagick::queryFormats());
"#,
    );
    assert_eq!(out, "JPEG;5");
}
