//! Purpose:
//! Tests for GD transforms and filters: region copies (plain, merge,
//! merge-gray, resized, resampled), `imagescale`, `imagecrop`/`imagecropauto`,
//! `imageflip`, `imagerotate`, `imageaffine`/`imageaffinematrixconcat`,
//! `imagefilter` (all `IMG_FILTER_*`), `imageconvolution`, `imagegammacorrect`,
//! and the interpolation/interlace/antialias accessors.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Results are checked by reading pixels back with `imagecolorat` (opaque colors
//!   pack to an exact GD integer) and by `imagesx`/`imagesy` for size-changing
//!   ops, so the assertions do not depend on encoder output. A fully transparent
//!   pixel packs to `2130706432` (GD alpha 127 in bits 24-30).
//! - `imagerotate` uses PHP's counter-clockwise convention; the 90° case is an
//!   exact permutation and is asserted against a known RGB strip.

use crate::support::*;

/// `imagecopy` blits a source region onto the destination at an offset, leaving
/// surrounding destination pixels untouched.
#[test]
fn test_imagecopy() {
    let out = compile_and_run(
        r#"<?php
$dst = imagecreatetruecolor(8, 8);
$blue = imagecolorallocate($dst, 0, 0, 255);
imagefilledrectangle($dst, 0, 0, 7, 7, $blue);
$src = imagecreatetruecolor(4, 4);
$red = imagecolorallocate($src, 255, 0, 0);
imagefilledrectangle($src, 0, 0, 3, 3, $red);
imagecopy($dst, $src, 2, 2, 0, 0, 4, 4);
echo imagecolorat($dst, 3, 3) . "," . imagecolorat($dst, 0, 0) . "," . imagecolorat($dst, 7, 7);
"#,
    );
    assert_eq!(out, "16711680,255,255");
}

/// `imagecopy` is safe when the source and destination are the same image: the
/// region is cloned before the copy writes, so the read area is not corrupted.
#[test]
fn test_imagecopy_self() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(8, 8);
$red = imagecolorallocate($im, 255, 0, 0);
imagefilledrectangle($im, 0, 0, 3, 3, $red);
imagecopy($im, $im, 4, 4, 0, 0, 4, 4);
echo imagecolorat($im, 0, 0) . "," . imagecolorat($im, 5, 5);
"#,
    );
    assert_eq!(out, "16711680,16711680");
}

/// `imagecopyresized` scales a region with nearest-neighbour sampling; a solid
/// region stays solid at the enlarged size.
#[test]
fn test_imagecopyresized() {
    let out = compile_and_run(
        r#"<?php
$src = imagecreatetruecolor(2, 2);
$green = imagecolorallocate($src, 0, 255, 0);
imagefilledrectangle($src, 0, 0, 1, 1, $green);
$dst = imagecreatetruecolor(8, 8);
imagecopyresized($dst, $src, 0, 0, 0, 0, 8, 8, 2, 2);
echo imagecolorat($dst, 4, 4);
"#,
    );
    assert_eq!(out, "65280");
}

/// `imagecopyresampled` scales a region with bilinear sampling; a solid region
/// stays solid.
#[test]
fn test_imagecopyresampled() {
    let out = compile_and_run(
        r#"<?php
$src = imagecreatetruecolor(2, 2);
$green = imagecolorallocate($src, 0, 255, 0);
imagefilledrectangle($src, 0, 0, 1, 1, $green);
$dst = imagecreatetruecolor(8, 8);
imagecopyresampled($dst, $src, 0, 0, 0, 0, 8, 8, 2, 2);
echo imagecolorat($dst, 4, 4);
"#,
    );
    assert_eq!(out, "65280");
}

/// `imagecopymerge` linearly blends the source over the destination at a given
/// opacity percent (blue under red at 50% → mid magenta).
#[test]
fn test_imagecopymerge() {
    let out = compile_and_run(
        r#"<?php
$dst = imagecreatetruecolor(4, 4);
imagefilledrectangle($dst, 0, 0, 3, 3, imagecolorallocate($dst, 0, 0, 255));
$src = imagecreatetruecolor(4, 4);
imagefilledrectangle($src, 0, 0, 3, 3, imagecolorallocate($src, 255, 0, 0));
imagecopymerge($dst, $src, 0, 0, 0, 0, 4, 4, 50);
echo imagecolorat($dst, 0, 0);
"#,
    );
    assert_eq!(out, "8388736");
}

/// `imagecopymergegray` desaturates the destination before merging; at 0% opacity
/// the result is just the grayed destination (red 200 → luminance 60).
#[test]
fn test_imagecopymergegray() {
    let out = compile_and_run(
        r#"<?php
$dst = imagecreatetruecolor(4, 4);
imagefilledrectangle($dst, 0, 0, 3, 3, imagecolorallocate($dst, 200, 0, 0));
$src = imagecreatetruecolor(4, 4);
imagefilledrectangle($src, 0, 0, 3, 3, imagecolorallocate($src, 0, 0, 200));
imagecopymergegray($dst, $src, 0, 0, 0, 0, 4, 4, 0);
echo imagecolorat($dst, 0, 0);
"#,
    );
    assert_eq!(out, "3947580");
}

/// `imagescale` with nearest mode resizes the whole image and keeps a solid color.
#[test]
fn test_imagescale_nearest() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
imagefilledrectangle($im, 0, 0, 3, 3, imagecolorallocate($im, 255, 0, 0));
$big = imagescale($im, 8, 8, IMG_NEAREST_NEIGHBOUR);
echo imagesx($big) . "x" . imagesy($big) . ":" . imagecolorat($big, 4, 4);
"#,
    );
    assert_eq!(out, "8x8:16711680");
}

/// `imagescale` with a negative height preserves the aspect ratio (8×4 → 4×2).
#[test]
fn test_imagescale_aspect() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(8, 4);
$small = imagescale($im, 4);
echo imagesx($small) . "x" . imagesy($small);
"#,
    );
    assert_eq!(out, "4x2");
}

/// `imagecrop` extracts a sub-rectangle into a new image of the requested size.
#[test]
fn test_imagecrop() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(8, 8);
imagefilledrectangle($im, 0, 0, 7, 7, imagecolorallocate($im, 0, 0, 255));
imagefilledrectangle($im, 2, 2, 5, 5, imagecolorallocate($im, 255, 0, 0));
$c = imagecrop($im, ["x" => 2, "y" => 2, "width" => 4, "height" => 4]);
echo imagesx($c) . "x" . imagesy($c) . ":" . imagecolorat($c, 0, 0) . "," . imagecolorat($c, 3, 3);
"#,
    );
    assert_eq!(out, "4x4:16711680,16711680");
}

/// `imagecrop` past the source edge fills the out-of-range area transparent.
#[test]
fn test_imagecrop_out_of_bounds() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
imagefilledrectangle($im, 0, 0, 3, 3, imagecolorallocate($im, 255, 0, 0));
$c = imagecrop($im, ["x" => 2, "y" => 2, "width" => 4, "height" => 4]);
echo imagecolorat($c, 0, 0) . "," . imagecolorat($c, 2, 2);
"#,
    );
    assert_eq!(out, "16711680,2130706432");
}

/// `imagecropauto` in white mode trims a uniform white border to the colored box.
#[test]
fn test_imagecropauto_white() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(6, 6);
imagefilledrectangle($im, 0, 0, 5, 5, imagecolorallocate($im, 255, 255, 255));
imagefilledrectangle($im, 2, 2, 3, 3, imagecolorallocate($im, 255, 0, 0));
$c = imagecropauto($im, IMG_CROP_WHITE);
echo imagesx($c) . "x" . imagesy($c) . ":" . imagecolorat($c, 0, 0);
"#,
    );
    assert_eq!(out, "2x2:16711680");
}

/// `imageflip` horizontal swaps columns left-to-right.
#[test]
fn test_imageflip_horizontal() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 1);
imagesetpixel($im, 0, 0, imagecolorallocate($im, 255, 0, 0));
imagesetpixel($im, 1, 0, imagecolorallocate($im, 0, 255, 0));
imageflip($im, IMG_FLIP_HORIZONTAL);
echo imagecolorat($im, 0, 0) . "," . imagecolorat($im, 1, 0);
"#,
    );
    assert_eq!(out, "65280,16711680");
}

/// `imageflip` vertical swaps rows top-to-bottom.
#[test]
fn test_imageflip_vertical() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(1, 2);
imagesetpixel($im, 0, 0, imagecolorallocate($im, 255, 0, 0));
imagesetpixel($im, 0, 1, imagecolorallocate($im, 0, 255, 0));
imageflip($im, IMG_FLIP_VERTICAL);
echo imagecolorat($im, 0, 0) . "," . imagecolorat($im, 0, 1);
"#,
    );
    assert_eq!(out, "65280,16711680");
}

/// `imagerotate` by 180° maps a corner pixel to the opposite corner; size is
/// unchanged.
#[test]
fn test_imagerotate_180() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(3, 2);
imagesetpixel($im, 0, 0, imagecolorallocate($im, 255, 0, 0));
$r = imagerotate($im, 180.0, 0);
echo imagesx($r) . "x" . imagesy($r) . ":" . imagecolorat($r, 2, 1) . "," . imagecolorat($r, 0, 0);
"#,
    );
    assert_eq!(out, "3x2:16711680,0");
}

/// `imagerotate` by 90° counter-clockwise swaps the dimensions and sends the right
/// end of the top row to the top of the result (PHP's CCW convention).
#[test]
fn test_imagerotate_90_ccw() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(3, 1);
imagesetpixel($im, 0, 0, imagecolorallocate($im, 255, 0, 0));
imagesetpixel($im, 2, 0, imagecolorallocate($im, 0, 0, 255));
$r = imagerotate($im, 90.0, 0);
echo imagesx($r) . "x" . imagesy($r) . ":" . imagecolorat($r, 0, 0) . "," . imagecolorat($r, 0, 2);
"#,
    );
    assert_eq!(out, "1x3:255,16711680");
}

/// `imagerotate` by a non-right angle enlarges the canvas and fills exposed corners
/// with the background color.
#[test]
fn test_imagerotate_45_background() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
imagefilledrectangle($im, 0, 0, 3, 3, imagecolorallocate($im, 255, 0, 0));
$blue = imagecolorallocate($im, 0, 0, 255);
$r = imagerotate($im, 45.0, $blue);
echo imagesx($r) . ":" . imagecolorat($r, 0, 0);
"#,
    );
    assert_eq!(out, "6:255");
}

/// `imageaffine` with a scale matrix doubles the canvas and keeps the color.
#[test]
fn test_imageaffine_scale() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
imagefilledrectangle($im, 0, 0, 3, 3, imagecolorallocate($im, 255, 0, 0));
$a = imageaffine($im, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
echo imagesx($a) . "x" . imagesy($a) . ":" . imagecolorat($a, 0, 0) . "," . imagecolorat($a, 7, 7);
"#,
    );
    assert_eq!(out, "8x8:16711680,16711680");
}

/// `imageaffine` throws `ImageException` for a singular (non-invertible) matrix.
#[test]
fn test_imageaffine_singular_throws() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
try {
    $a = imageaffine($im, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    echo "nothrow";
} catch (ImageException $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
}

/// `imageaffinematrixconcat` composes two affine matrices (identity ∘ translate
/// keeps the translation).
#[test]
fn test_imageaffinematrixconcat() {
    let out = compile_and_run(
        r#"<?php
$r = imageaffinematrixconcat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0, 5.0, 7.0]);
$a = (int) $r[0];
$e = (int) $r[4];
$f = (int) $r[5];
echo $a . "_" . $e . "_" . $f;
"#,
    );
    assert_eq!(out, "1_5_7");
}

/// `imagefilter`/`IMG_FILTER_NEGATE` inverts each RGB channel.
#[test]
fn test_imagefilter_negate() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
imagefilledrectangle($im, 0, 0, 1, 1, imagecolorallocate($im, 10, 20, 30));
imagefilter($im, IMG_FILTER_NEGATE);
echo imagecolorat($im, 0, 0);
"#,
    );
    assert_eq!(out, "16116705");
}

/// `imagefilter`/`IMG_FILTER_GRAYSCALE` collapses to luminance.
#[test]
fn test_imagefilter_grayscale() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
imagefilledrectangle($im, 0, 0, 1, 1, imagecolorallocate($im, 100, 150, 200));
imagefilter($im, IMG_FILTER_GRAYSCALE);
echo imagecolorat($im, 0, 0);
"#,
    );
    assert_eq!(out, "9276813");
}

/// `imagefilter`/`IMG_FILTER_BRIGHTNESS` adds a per-channel offset.
#[test]
fn test_imagefilter_brightness() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
imagefilledrectangle($im, 0, 0, 1, 1, imagecolorallocate($im, 10, 20, 30));
imagefilter($im, IMG_FILTER_BRIGHTNESS, 50);
echo imagecolorat($im, 0, 0);
"#,
    );
    assert_eq!(out, "3950160");
}

/// `imagefilter`/`IMG_FILTER_COLORIZE` adds independent RGB offsets.
#[test]
fn test_imagefilter_colorize() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
imagefilledrectangle($im, 0, 0, 1, 1, imagecolorallocate($im, 10, 20, 30));
imagefilter($im, IMG_FILTER_COLORIZE, 5, 5, 5);
echo imagecolorat($im, 0, 0);
"#,
    );
    assert_eq!(out, "989475");
}

/// `imagefilter`/`IMG_FILTER_PIXELATE` (average) replaces a block with its mean.
#[test]
fn test_imagefilter_pixelate() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
imagefilledrectangle($im, 0, 0, 1, 3, imagecolorallocate($im, 255, 0, 0));
imagefilledrectangle($im, 2, 0, 3, 3, imagecolorallocate($im, 0, 0, 255));
imagefilter($im, IMG_FILTER_PIXELATE, 4, 1);
echo imagecolorat($im, 0, 0) . "," . imagecolorat($im, 3, 3);
"#,
    );
    assert_eq!(out, "8323199,8323199");
}

/// `imageconvolution` applies an arbitrary kernel; a center weight of 2 doubles a
/// uniform image's channels.
#[test]
fn test_imageconvolution() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(3, 3);
imagefilledrectangle($im, 0, 0, 2, 2, imagecolorallocate($im, 10, 20, 30));
imageconvolution($im, [[0, 0, 0], [0, 2, 0], [0, 0, 0]], 1.0, 0.0);
echo imagecolorat($im, 1, 1);
"#,
    );
    assert_eq!(out, "1321020");
}

/// `imagegammacorrect` with equal input/output gamma leaves the image unchanged.
#[test]
fn test_imagegammacorrect_identity() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
imagefilledrectangle($im, 0, 0, 1, 1, imagecolorallocate($im, 100, 100, 100));
imagegammacorrect($im, 2.0, 2.0);
echo imagecolorat($im, 0, 0);
"#,
    );
    assert_eq!(out, "6579300");
}

/// `imagesetinterpolation`/`imagegetinterpolation` round-trip the method, with
/// GD's `IMG_BILINEAR_FIXED` (3) default.
#[test]
fn test_image_interpolation() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
$before = imagegetinterpolation($im);
imagesetinterpolation($im, IMG_BICUBIC);
echo $before . "," . imagegetinterpolation($im);
"#,
    );
    assert_eq!(out, "3,4");
}

/// `imageinterlace` sets and queries the interlace flag, returning the bit.
#[test]
fn test_image_interlace() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
$on = imageinterlace($im, true);
$q = imageinterlace($im);
$off = imageinterlace($im, false);
echo $on . "," . $q . "," . $off;
"#,
    );
    assert_eq!(out, "1,1,0");
}

/// `imageantialias` is accepted as a no-op and returns true.
#[test]
fn test_imageantialias() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
echo imageantialias($im, true) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}
