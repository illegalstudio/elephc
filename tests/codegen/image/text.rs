//! Purpose:
//! Tests for GD built-in bitmap text: `imagestring`, `imagestringup`,
//! `imagechar`, and the `imagefontwidth`/`imagefontheight` metrics.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests count colored pixels in a glyph cell rather than asserting exact glyph
//!   bitmaps, so they verify that text is rendered in the right place without
//!   depending on the specific `font8x8` glyph shapes (white = 16777215).
//! - All built-in fonts use a uniform 8×8 cell in elephc.

use crate::support::*;

/// `imagestring` renders glyphs into the 8×8 cell at the origin (some pixels are
/// colored) and leaves distant background untouched.
#[test]
fn test_imagestring() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(16, 16);
$w = imagecolorallocate($im, 255, 255, 255);
imagestring($im, 3, 0, 0, "A", $w);
$count = 0;
for ($y = 0; $y < 8; $y++) {
    for ($x = 0; $x < 8; $x++) {
        if (imagecolorat($im, $x, $y) === 16777215) {
            $count++;
        }
    }
}
echo "drawn=" . ($count > 0 ? "1" : "0") . " corner=" . imagecolorat($im, 15, 15);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "drawn=1 corner=0");
}

/// `imagefontwidth`/`imagefontheight` report the uniform 8×8 built-in cell.
#[test]
fn test_font_metrics() {
    let out = compile_and_run(
        r#"<?php
echo imagefontwidth(3) . "x" . imagefontheight(5);
"#,
    );
    assert_eq!(out, "8x8");
}

/// `imagechar` draws only the first character of the string: the first cell has
/// colored pixels, the second cell stays empty.
#[test]
fn test_imagechar_first_only() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(16, 16);
$w = imagecolorallocate($im, 255, 255, 255);
imagechar($im, 3, 0, 0, "XYZ", $w);
$first = 0;
$second = 0;
for ($y = 0; $y < 8; $y++) {
    for ($x = 0; $x < 8; $x++) {
        if (imagecolorat($im, $x, $y) === 16777215) {
            $first++;
        }
        if (imagecolorat($im, $x + 8, $y) === 16777215) {
            $second++;
        }
    }
}
echo "first=" . ($first > 0 ? "1" : "0") . " second=" . ($second > 0 ? "1" : "0");
imagedestroy($im);
"#,
    );
    assert_eq!(out, "first=1 second=0");
}

/// `imagestringup` renders the rotated glyph into the cell above the origin.
#[test]
fn test_imagestringup() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(16, 16);
$w = imagecolorallocate($im, 255, 255, 255);
imagestringup($im, 3, 0, 15, "A", $w);
$count = 0;
for ($y = 8; $y < 16; $y++) {
    for ($x = 0; $x < 8; $x++) {
        if (imagecolorat($im, $x, $y) === 16777215) {
            $count++;
        }
    }
}
echo ($count > 0 ? "up_ok" : "up_empty");
imagedestroy($im);
"#,
    );
    assert_eq!(out, "up_ok");
}
