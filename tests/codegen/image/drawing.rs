//! Purpose:
//! Tests for GD drawing and fill primitives: lines (plain, dashed, and
//! thick), rectangles (outline and filled), ellipses, arcs (pie fill), polygons,
//! and flood fill. Behavior is verified by drawing a shape and reading back
//! specific pixels with `imagecolorat`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Drawing uses opaque colors so `imagecolorat` returns the exact packed value
//!   on covered pixels and `0` (opaque black) on untouched background.
//! - Packed color references: white = 16777215, green = 65280, red = 16711680,
//!   blue = 255, yellow = 16776960.

use crate::support::*;

/// `imageline` draws along the segment: a pixel on the horizontal line reads as
/// the drawn color, one off it stays background.
#[test]
fn test_imageline() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(10, 10);
$c = imagecolorallocate($im, 255, 255, 255);
imageline($im, 0, 5, 9, 5, $c);
echo "on=" . imagecolorat($im, 4, 5) . " off=" . imagecolorat($im, 4, 0);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "on=16777215 off=0");
}

/// `imagefilledrectangle` fills its interior; `imagerectangle` draws only the
/// outline (interior stays background).
#[test]
fn test_rectangles() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(10, 10);
$green = imagecolorallocate($im, 0, 255, 0);
imagefilledrectangle($im, 2, 2, 7, 7, $green);
echo "fill_in=" . imagecolorat($im, 5, 5) . " fill_out=" . imagecolorat($im, 0, 0) . "\n";

$im2 = imagecreatetruecolor(10, 10);
imagerectangle($im2, 2, 2, 7, 7, $green);
echo "edge=" . imagecolorat($im2, 2, 5) . " inside=" . imagecolorat($im2, 5, 5) . "\n";
imagedestroy($im);
imagedestroy($im2);
"#,
    );
    assert_eq!(out, "fill_in=65280 fill_out=0\nedge=65280 inside=0\n");
}

/// `imagefilledellipse` fills toward the center while leaving the corners of the
/// bounding box untouched.
#[test]
fn test_filled_ellipse() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(20, 20);
$red = imagecolorallocate($im, 255, 0, 0);
imagefilledellipse($im, 10, 10, 16, 16, $red);
echo "center=" . imagecolorat($im, 10, 10) . " corner=" . imagecolorat($im, 0, 0);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "center=16711680 corner=0");
}

/// `imagefilledpolygon` fills the interior of a triangle given as a flat point
/// array.
#[test]
fn test_filled_polygon() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(20, 20);
$blue = imagecolorallocate($im, 0, 0, 255);
$points = [10, 2, 2, 18, 18, 18];
imagefilledpolygon($im, $points, $blue);
echo "in=" . imagecolorat($im, 10, 12) . " out=" . imagecolorat($im, 0, 0);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "in=255 out=0");
}

/// `imagefill` flood-fills a uniform image from a seed point, recoloring every
/// connected pixel.
#[test]
fn test_flood_fill() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(10, 10);
$yellow = imagecolorallocate($im, 255, 255, 0);
imagefill($im, 0, 0, $yellow);
echo imagecolorat($im, 5, 5);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "16776960");
}

/// `imagesetthickness` widens the stroke: a thick vertical line covers the
/// adjacent column but not pixels farther away.
#[test]
fn test_set_thickness() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(10, 10);
$w = imagecolorallocate($im, 255, 255, 255);
imagesetthickness($im, 3);
imageline($im, 5, 0, 5, 9, $w);
echo "center=" . imagecolorat($im, 5, 5)
    . " adj=" . imagecolorat($im, 4, 5)
    . " far=" . imagecolorat($im, 2, 5);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "center=16777215 adj=16777215 far=0");
}

/// `imagefilledarc` with `IMG_ARC_PIE` fills the angular sector: a point inside
/// the 0–90° sweep is colored, one in the opposite quadrant is not.
#[test]
fn test_filled_arc_pie() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(20, 20);
$red = imagecolorallocate($im, 255, 0, 0);
imagefilledarc($im, 10, 10, 16, 16, 0, 90, $red, IMG_ARC_PIE);
echo "in=" . imagecolorat($im, 13, 13) . " out=" . imagecolorat($im, 7, 7);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "in=16711680 out=0");
}

/// `imagedashedline` leaves gaps: the first pixels of the dash are drawn while a
/// pixel in the off-segment stays background.
#[test]
fn test_dashed_line() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(20, 4);
$w = imagecolorallocate($im, 255, 255, 255);
imagedashedline($im, 0, 1, 19, 1, $w);
echo "start=" . imagecolorat($im, 0, 1) . " gap=" . imagecolorat($im, 8, 1);
imagedestroy($im);
"#,
    );
    // Dash pattern is 6 on / 6 off: x=0 is on, x=8 falls in the first off-run.
    assert_eq!(out, "start=16777215 gap=0");
}
