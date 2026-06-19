//! Purpose:
//! Foundation tests for PHP image support: the always-available core
//! functions (`getimagesize`, `image_type_to_mime_type`,
//! `image_type_to_extension`) and a minimal GD raster round-trip
//! (create true-color → allocate color → set pixel → query size → write PNG →
//! probe the written file).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures write to a unique `tempnam()` path under the system temp dir so
//!   parallel test processes never collide, and `unlink()` cleans up afterward.
//! - The round-trip's `getimagesize()` read-back is a strong end-to-end check: it
//!   only succeeds if the bridge actually encoded a valid PNG of the right size.

use crate::support::*;

/// Creating a true-color image, drawing a pixel, writing it as PNG, and probing
/// the written file round-trips through the `elephc_image` bridge: the size is
/// reported correctly and `getimagesize` reads back the PNG's dimensions, type
/// (`IMAGETYPE_PNG` = 3), and MIME.
#[test]
fn test_image_png_round_trip() {
    let out = compile_and_run(
        r#"<?php
$path = (string) tempnam(sys_get_temp_dir(), "elephc_img_p0_");
$im = imagecreatetruecolor(4, 3);
$red = imagecolorallocate($im, 255, 0, 0);
imagesetpixel($im, 1, 1, $red);
echo "size=" . imagesx($im) . "x" . imagesy($im) . "\n";
$ok = imagepng($im, $path);
echo "png_ok=" . ($ok ? "1" : "0") . "\n";
$info = getimagesize($path);
echo "probe=" . $info[0] . "x" . $info[1] . " type=" . $info[2] . " mime=" . $info["mime"] . "\n";
imagedestroy($im);
unlink($path);
"#,
    );
    assert_eq!(
        out,
        "size=4x3\npng_ok=1\nprobe=4x3 type=3 mime=image/png\n"
    );
}

/// `image_type_to_mime_type` maps `IMAGETYPE_*` codes to their PHP MIME strings.
#[test]
fn test_image_type_to_mime_type() {
    let out = compile_and_run(
        r#"<?php
echo image_type_to_mime_type(IMAGETYPE_PNG) . "\n";
echo image_type_to_mime_type(IMAGETYPE_JPEG) . "\n";
echo image_type_to_mime_type(IMAGETYPE_GIF) . "\n";
echo image_type_to_mime_type(IMAGETYPE_WEBP) . "\n";
"#,
    );
    assert_eq!(out, "image/png\nimage/jpeg\nimage/gif\nimage/webp\n");
}

/// `image_type_to_extension` returns the extension with the leading dot by
/// default and without it when `$include_dot` is false.
///
/// Known limitation: PHP returns `false` for an unknown type, but elephc
/// currently collapses a `string|false` function return to `string`, so an
/// unknown type yields "" here (asserted as the empty third line). This is
/// tracked with the scalar-union value-runtime work; revisit when fixed.
#[test]
fn test_image_type_to_extension() {
    let out = compile_and_run(
        r#"<?php
echo image_type_to_extension(IMAGETYPE_PNG) . "\n";
echo image_type_to_extension(IMAGETYPE_JPEG, false) . "\n";
echo image_type_to_extension(IMAGETYPE_UNKNOWN) . "\n";
"#,
    );
    assert_eq!(out, ".png\njpeg\n\n");
}

/// `imagesx`/`imagesy` report the dimensions of a freshly created image without
/// needing any file I/O.
#[test]
fn test_image_dimensions() {
    let out = compile_and_run(
        r#"<?php
$im = imagecreatetruecolor(640, 480);
echo imagesx($im) . "x" . imagesy($im);
imagedestroy($im);
"#,
    );
    assert_eq!(out, "640x480");
}

/// `getimagesize` returns `false` for a path that does not resolve to a readable
/// image, matching PHP.
#[test]
fn test_getimagesize_missing_file_returns_false() {
    let out = compile_and_run(
        r#"<?php
$info = getimagesize("/nonexistent/elephc-image-does-not-exist.png");
echo ($info === false ? "false" : "array");
"#,
    );
    assert_eq!(out, "false");
}
