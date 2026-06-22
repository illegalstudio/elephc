//! Purpose:
//! Tests for GD color handling: reading pixels (`imagecolorat`),
//! unpacking colors (`imagecolorsforindex`), the transparent color, the palette
//! color count, alpha blending vs. replacement in `imagesetpixel`, alpha
//! persistence on encode (`imagesavealpha`), palette↔true-color flips, and the
//! no-op palette-entry stubs `imagecolorset` / `imagepalettecopy`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every elephc image is true-color RGBA, so palette-specific behavior is
//!   approximated; tests assert the observable GD-level results.
//! - GD's 7-bit alpha round-trips through 8-bit storage with ±1 rounding, so
//!   alpha assertions use opaque values (exact) or `> 0` thresholds rather than
//!   exact mid-range alpha equality.

use crate::support::*;

/// `imagecolorat` reads back the exact opaque color written by `imagesetpixel`
/// (blending an opaque color is a replace), and the untouched background reads as
/// opaque black (`0`).
#[test]
fn test_imagecolorat_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(3, 3);
$orange = imagecolorallocate($im, 255, 128, 0);
imagesetpixel($im, 1, 1, $orange);
echo "px=" . imagecolorat($im, 1, 1) . "\n";
echo "bg=" . imagecolorat($im, 0, 0) . "\n";
imagedestroy($im);
"#,
    );
    // (255<<16)|(128<<8)|0 = 16744448; opaque black = 0.
    assert_eq!(out, "px=16744448\nbg=0\n");
}

/// `imagecolorsforindex` unpacks a GD packed color into red/green/blue and the
/// 7-bit alpha component.
#[test]
fn test_imagecolorsforindex() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(1, 1);
$c = imagecolorallocatealpha($im, 10, 20, 30, 64);
$parts = imagecolorsforindex($im, $c);
echo $parts["red"] . "," . $parts["green"] . "," . $parts["blue"] . "," . $parts["alpha"];
imagedestroy($im);
"#,
    );
    assert_eq!(out, "10,20,30,64");
}

/// `imagecolortransparent` returns `-1` by default, returns the color it is set
/// to, and reports that color when queried again.
#[test]
fn test_imagecolortransparent_get_set() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
echo "def=" . imagecolortransparent($im) . "\n";
$red = imagecolorallocate($im, 255, 0, 0);
echo "set=" . imagecolortransparent($im, $red) . "\n";
echo "get=" . imagecolortransparent($im) . "\n";
imagedestroy($im);
"#,
    );
    assert_eq!(out, "def=-1\nset=16711680\nget=16711680\n");
}

/// `imagecolorstotal` reports `0` for a true-color image, matching GD.
#[test]
fn test_imagecolorstotal_truecolor() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(8, 8);
echo imagecolorstotal($im);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "0");
}

/// With alpha blending off, a semi-transparent pixel keeps its alpha; with
/// blending on, compositing it over the opaque black background yields an opaque
/// pixel whose red is the alpha-weighted blend.
#[test]
fn test_alphablending_replace_vs_blend() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
$semi = imagecolorallocatealpha($im, 255, 0, 0, 64);

imagealphablending($im, false);
imagesetpixel($im, 0, 0, $semi);
$a = imagecolorsforindex($im, imagecolorat($im, 0, 0));
echo "off_alpha>0=" . ($a["alpha"] > 0 ? "1" : "0") . "\n";

imagealphablending($im, true);
imagesetpixel($im, 1, 1, $semi);
$b = imagecolorsforindex($im, imagecolorat($im, 1, 1));
echo "on_alpha=" . $b["alpha"] . " on_red=" . $b["red"] . "\n";
imagedestroy($im);
"#,
    );
    assert_eq!(out, "off_alpha>0=1\non_alpha=0 on_red=127\n");
}

/// `imagesavealpha(true)` preserves the alpha channel through a PNG encode/decode
/// round-trip; the default (off) flattens it to opaque.
#[test]
fn test_imagesavealpha_roundtrip() {
    let out = compile_and_run(
        r#"<?php
function write_and_reload(bool $save): int {
    $im = imagecreatetruecolor(2, 2);
    imagealphablending($im, false);
    imagesavealpha($im, $save);
    $semi = imagecolorallocatealpha($im, 0, 0, 255, 100);
    imagesetpixel($im, 0, 0, $semi);
    $path = (string) tempnam(sys_get_temp_dir(), "elephc_img_sa_");
    imagepng($im, $path);
    $im2 = imagecreatefromstring((string) file_get_contents($path));
    $parts = imagecolorsforindex($im2, imagecolorat($im2, 0, 0));
    unlink($path);
    imagedestroy($im);
    imagedestroy($im2);
    return $parts["alpha"];
}
echo "saved>0=" . (write_and_reload(true) > 0 ? "1" : "0") . "\n";
echo "flattened=" . write_and_reload(false) . "\n";
"#,
    );
    assert_eq!(out, "saved>0=1\nflattened=0\n");
}

/// `imagepalettetotruecolor` / `imagetruecolortopalette` flip the true-color flag
/// reported by `imageistruecolor`.
#[test]
fn test_palette_truecolor_flip() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreate(4, 4);
echo "start=" . (imageistruecolor($im) ? "tc" : "pal") . "\n";
imagepalettetotruecolor($im);
echo "toTrue=" . (imageistruecolor($im) ? "tc" : "pal") . "\n";
imagetruecolortopalette($im, false, 256);
echo "toPal=" . (imageistruecolor($im) ? "tc" : "pal") . "\n";
imagedestroy($im);
"#,
    );
    assert_eq!(out, "start=pal\ntoTrue=tc\ntoPal=pal\n");
}

/// `imagecolorset` is accepted as a no-op success: elephc stores every image as
/// true-color RGBA with no palette slots to recolor, so the call reports `true`
/// (documented limitation) without raising.
#[test]
fn test_imagecolorset_noop_returns_true() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(2, 2);
$red = imagecolorallocate($im, 255, 0, 0);
echo "colorset=" . (imagecolorset($im, $red, 0, 0, 255) ? "1" : "0");
imagedestroy($im);
"#,
    );
    assert_eq!(out, "colorset=1");
}

/// `imagepalettecopy` is accepted as a no-op success for the same reason: there
/// is no palette to copy between two RGBA images, so it reports `true`.
#[test]
fn test_imagepalettecopy_noop_returns_true() {
    let out = compile_and_run(
        r#"<?php
$dst = imagecreatetruecolor(2, 2);
$src = imagecreatetruecolor(2, 2);
echo "palettecopy=" . (imagepalettecopy($dst, $src) ? "1" : "0");
imagedestroy($dst);
imagedestroy($src);
"#,
    );
    assert_eq!(out, "palettecopy=1");
}
