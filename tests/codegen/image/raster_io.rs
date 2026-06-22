//! Purpose:
//! Tests for GD raster I/O: encoding to and decoding from PNG/JPEG/GIF/
//! BMP/WebP/TGA (both file and in-memory string), per-format `imagecreatefrom*`
//! decoders with format enforcement, the binary blob ABI used for
//! `imagecreatefromstring` and the no-file output path, the in-memory probe for
//! `getimagesizefromstring`, and the GD info functions (`imageistruecolor`,
//! `imageresolution`, `imagetypes`, `gd_info`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Encoded image bytes are never echoed to stdout (the harness decodes stdout
//!   as strict UTF-8 and would panic on binary). Output is verified via temp
//!   files and `getimagesize`, and the in-memory encode path is verified by
//!   re-decoding the bytes with `imagecreatefromstring` rather than printing them.
//! - Fixtures use unique `tempnam()` paths so parallel test processes never
//!   collide, and `unlink()` cleans up.

use crate::support::*;

/// A PNG written to a file, read back with `file_get_contents`, and decoded with
/// `imagecreatefromstring` round-trips: the bytes are non-empty and the decoded
/// image reports the original dimensions. Exercises the file output path and the
/// staging-buffer input blob ABI.
#[test]
fn test_png_string_round_trip() {
    let out = compile_and_run(
        r#"<?php
$path = (string) tempnam(sys_get_temp_dir(), "elephc_img_p1png_");
$im = imagecreatetruecolor(7, 5);
imagepng($im, $path);
$data = (string) file_get_contents($path);
echo "len>0=" . (strlen($data) > 0 ? "1" : "0") . "\n";
$im2 = imagecreatefromstring($data);
echo "redecode=" . imagesx($im2) . "x" . imagesy($im2) . "\n";
imagedestroy($im);
imagedestroy($im2);
unlink($path);
"#,
    );
    assert_eq!(out, "len>0=1\nredecode=7x5\n");
}

/// `imagejpeg` writes a file that `getimagesize` recognizes as JPEG (type 2) and
/// `imagecreatefromjpeg` decodes back to the original dimensions.
#[test]
fn test_jpeg_file_round_trip() {
    let out = compile_and_run(
        r#"<?php
$path = (string) tempnam(sys_get_temp_dir(), "elephc_img_p1jpg_");
$im = imagecreatetruecolor(16, 9);
imagejpeg($im, $path, 80);
$info = getimagesize($path);
echo "type=" . $info[2] . " mime=" . $info["mime"] . "\n";
$im2 = imagecreatefromjpeg($path);
echo "dims=" . imagesx($im2) . "x" . imagesy($im2) . "\n";
imagedestroy($im);
imagedestroy($im2);
unlink($path);
"#,
    );
    assert_eq!(out, "type=2 mime=image/jpeg\ndims=16x9\n");
}

/// GIF, BMP, and WebP each survive a write-then-`getimagesize`-then-decode
/// round-trip with the correct IMAGETYPE_* and dimensions, covering the rest of
/// the raster I/O codec set.
#[test]
fn test_gif_bmp_webp_round_trip() {
    let out = compile_and_run(
        r#"<?php
function rt(string $tag, string $path): void {
    $info = getimagesize($path);
    echo $tag . "=" . $info[2] . "\n";
}
$gif = (string) tempnam(sys_get_temp_dir(), "elephc_img_gif_");
$bmp = (string) tempnam(sys_get_temp_dir(), "elephc_img_bmp_");
$webp = (string) tempnam(sys_get_temp_dir(), "elephc_img_webp_");
$im = imagecreatetruecolor(8, 8);
imagegif($im, $gif);
imagebmp($im, $bmp);
imagewebp($im, $webp);
rt("gif", $gif);
rt("bmp", $bmp);
rt("webp", $webp);
$g = imagecreatefromgif($gif);
$b = imagecreatefrombmp($bmp);
$w = imagecreatefromwebp($webp);
echo "decoded=" . imagesx($g) . imagesx($b) . imagesx($w) . "\n";
imagedestroy($im);
unlink($gif);
unlink($bmp);
unlink($webp);
"#,
    );
    assert_eq!(out, "gif=1\nbmp=6\nwebp=18\ndecoded=888\n");
}

/// `imagecreatefromjpeg` on a PNG file fails the format check and throws an
/// `ImageException`: the per-format decoders enforce the expected format. (PHP
/// returns `false`; elephc throws — see the prelude's compatibility note.)
#[test]
fn test_imagecreatefrom_format_mismatch_throws() {
    let out = compile_and_run(
        r#"<?php
$path = (string) tempnam(sys_get_temp_dir(), "elephc_img_mm_");
$im = imagecreatetruecolor(4, 4);
imagepng($im, $path);
try {
    $bad = imagecreatefromjpeg($path);
    echo "image:" . imagesx($bad);
} catch (ImageException $e) {
    echo "threw";
}
imagedestroy($im);
unlink($path);
"#,
    );
    assert_eq!(out, "threw");
}

/// The in-memory encode path (used by the no-file output form) produces valid
/// bytes: encoding into the bridge cell, copying them out with `ptr_read_string`,
/// and re-decoding with `imagecreatefromstring` reproduces the dimensions. This
/// verifies the encode-cell mechanism without echoing binary to stdout.
#[test]
fn test_encode_cell_blob_path() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(6, 3);
if (elephc_img_encode($im->handle, 1, -1) !== 0) {
    echo "encode_failed";
} else {
    $len = elephc_img_encoded_len();
    $ptr = elephc_img_encoded_ptr();
    $bytes = ptr_read_string($ptr, $len);
    elephc_img_encoded_clear();
    $im2 = imagecreatefromstring($bytes);
    echo "ok=" . imagesx($im2) . "x" . imagesy($im2);
    imagedestroy($im2);
}
imagedestroy($im);
"#,
    );
    assert_eq!(out, "ok=6x3");
}

/// `imageistruecolor` distinguishes a true-color image (`imagecreatetruecolor`)
/// from a palette image (`imagecreate`).
#[test]
fn test_imageistruecolor() {
    let out = compile_and_run(
        r#"<?php
$tc = imagecreatetruecolor(4, 4);
$pal = imagecreate(4, 4);
echo (imageistruecolor($tc) ? "tc" : "pal") . "/" . (imageistruecolor($pal) ? "tc" : "pal");
imagedestroy($tc);
imagedestroy($pal);
"#,
    );
    assert_eq!(out, "tc/pal");
}

/// `imageresolution` returns the default 96-DPI pair, and after setting a new
/// resolution returns the stored values.
#[test]
fn test_imageresolution_get_set() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(4, 4);
$def = imageresolution($im);
echo "def=" . $def[0] . "x" . $def[1] . "\n";
imageresolution($im, 300, 150);
$set = imageresolution($im);
echo "set=" . $set[0] . "x" . $set[1] . "\n";
imagedestroy($im);
"#,
    );
    assert_eq!(out, "def=96x96\nset=300x150\n");
}

/// `imagetypes` advertises the supported formats as a bitmask and `gd_info`
/// reports per-format capabilities; PNG/JPEG/GIF/WebP/BMP are supported and
/// FreeType is not (yet).
#[test]
fn test_imagetypes_and_gd_info() {
    let out = compile_and_run(
        r#"<?php
$t = imagetypes();
echo "png=" . (($t & IMG_PNG) ? "1" : "0");
echo " webp=" . (($t & IMG_WEBP) ? "1" : "0");
echo " xpm=" . (($t & IMG_XPM) ? "1" : "0") . "\n";
$info = gd_info();
echo "PNG=" . ($info["PNG Support"] ? "1" : "0");
echo " FreeType=" . ($info["FreeType Support"] ? "1" : "0") . "\n";
"#,
    );
    assert_eq!(out, "png=1 webp=1 xpm=0\nPNG=1 FreeType=0\n");
}

/// `imagecreatefromstring` throws an `ImageException` for bytes that are not a
/// recognized image. (PHP returns `false`; elephc throws — see the prelude's
/// compatibility note.)
#[test]
fn test_imagecreatefromstring_invalid_throws() {
    let out = compile_and_run(
        r#"<?php
try {
    $im = imagecreatefromstring("not an image at all");
    echo "image:" . imagesx($im);
} catch (ImageException $e) {
    echo "threw";
}
"#,
    );
    assert_eq!(out, "threw");
}

/// `getimagesizefromstring` probes bytes staged from a PHP string (here a PNG read
/// via `file_get_contents`) and returns the same array shape as `getimagesize`:
/// dimensions, IMAGETYPE_* code, and MIME. An empty string yields `false`.
#[test]
fn test_getimagesizefromstring() {
    let out = compile_and_run(
        r#"<?php
$path = (string) tempnam(sys_get_temp_dir(), "elephc_img_giss_");
$im = imagecreatetruecolor(7, 5);
imagepng($im, $path);
$data = (string) file_get_contents($path);
$info = getimagesizefromstring($data);
echo "dims=" . $info[0] . "x" . $info[1] . " type=" . $info[2] . " mime=" . $info["mime"] . "\n";
$empty = getimagesizefromstring("");
echo "empty=" . ($empty === false ? "false" : "array");
imagedestroy($im);
unlink($path);
"#,
    );
    assert_eq!(out, "dims=7x5 type=3 mime=image/png\nempty=false");
}

/// `imagecreatefromtga` decodes a hand-built uncompressed true-color TGA (whose
/// header has no sniffable magic, so the decoder is pinned by the requested
/// format), reporting the original dimensions and a decoded pixel. The fixture is
/// a 2×2 solid-red image stored as 24-bit BGR.
#[test]
fn test_imagecreatefromtga() {
    let out = compile_and_run(
        r#"<?php
function le16($n) { return chr($n & 0xFF) . chr(($n >> 8) & 0xFF); }
$w = 2; $h = 2;
// TGA header (18 bytes): no id, no color map, uncompressed true-color (type 2),
// 24-bit BGR pixels, bottom-left origin.
$header = chr(0) . chr(0) . chr(2)
    . le16(0) . le16(0) . chr(0)
    . le16(0) . le16(0)
    . le16($w) . le16($h)
    . chr(24) . chr(0);
$px = chr(0x00) . chr(0x00) . chr(0xFF);
$row = str_repeat($px, $w);
$tga = $header . str_repeat($row, $h);
$path = (string) tempnam(sys_get_temp_dir(), "elephc_img_tga_");
file_put_contents($path, $tga);
$im = imagecreatefromtga($path);
echo "dims=" . imagesx($im) . "x" . imagesy($im) . "\n";
echo "px=" . imagecolorat($im, 0, 0) . "\n";
imagedestroy($im);
unlink($path);
"#,
    );
    // 2×2; opaque red packs to (255<<16)|0|0 = 16711680.
    assert_eq!(out, "dims=2x2\npx=16711680\n");
}
