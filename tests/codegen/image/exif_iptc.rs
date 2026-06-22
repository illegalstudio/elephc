//! Purpose:
//! Tests for the Exif + IPTC surface: `exif_tagname`, `exif_imagetype`,
//! `exif_read_data` / `read_exif_data`, `exif_thumbnail`, `iptcparse`, and
//! `iptcembed`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures are built byte-by-byte in PHP (a minimal little-endian EXIF/TIFF
//!   APP1 segment, an IPTC IIM block) so the tests need no external image files
//!   and stay offline-deterministic. `HELPERS` provides the `le16`/`le32`/`be16`
//!   byte writers shared by the EXIF builders.
//! - EXIF field values come back as strings (ASCII text, SHORT as a decimal,
//!   RATIONAL as `num/den`) — elephc's documented simplification of PHP's typed
//!   EXIF values. `exif_tagname`/`exif_thumbnail` yield `""` (not `false`) on the
//!   not-found path because elephc collapses a `string|false` return to string.

use crate::support::*;

/// Little-endian / big-endian byte writers prepended to fixture-building tests.
const HELPERS: &str = r#"
function le16($n) { return chr($n & 0xFF) . chr(($n >> 8) & 0xFF); }
function le32($n) { return chr($n & 0xFF) . chr(($n >> 8) & 0xFF) . chr(($n >> 16) & 0xFF) . chr(($n >> 24) & 0xFF); }
function be16($n) { return chr(($n >> 8) & 0xFF) . chr($n & 0xFF); }
"#;

/// `exif_tagname` maps standard tag numbers to their EXIF mnemonics across the
/// TIFF, EXIF, and pointer spaces.
#[test]
fn test_exif_tagname() {
    let out = compile_and_run(
        r#"<?php
echo exif_tagname(0x010F), "|", exif_tagname(0x0112), "|", exif_tagname(0x8825), "|", exif_tagname(0x829A);
"#,
    );
    assert_eq!(out, "Make|Orientation|GPS_IFD_Pointer|ExposureTime");
}

/// An unknown tag yields "" (the documented `string|false` collapse), not false.
#[test]
fn test_exif_tagname_unknown_is_empty() {
    let out = compile_and_run(
        r#"<?php
$n = exif_tagname(0x9999);
echo ($n === "" ? "EMPTY" : "VALUE:" . $n);
"#,
    );
    assert_eq!(out, "EMPTY");
}

/// `exif_imagetype` returns the IMAGETYPE_* code for a real image and false for a
/// non-image or missing file (int|false keeps the false).
#[test]
fn test_exif_imagetype() {
    let out = compile_and_run(
        r#"<?php
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6it_");
$im = imagecreatetruecolor(4, 4);
imagepng($im, $p);
$png = exif_imagetype($p);
$txt = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6tx_");
file_put_contents($txt, "not an image at all");
$bad = exif_imagetype($txt);
$missing = exif_imagetype("/no/such/file.xyz");
echo $png, "|", ($bad === false ? "F" : $bad), "|", ($missing === false ? "F" : $missing);
"#,
    );
    assert_eq!(out, "3|F|F");
}

/// `exif_read_data` parses a hand-built EXIF JPEG, rendering ASCII, SHORT, and
/// RATIONAL fields as PHP-style strings.
#[test]
fn test_exif_read_data() {
    let src = format!(
        r#"<?php
{HELPERS}
// IFD0 with Make (ASCII), Orientation (SHORT), XResolution (RATIONAL @ offset 50).
$header = "II" . chr(0x2A) . chr(0x00) . le32(8);
$ifd0 = le16(3)
      . le16(0x010F) . le16(2) . le32(4) . "ACE\x00"             // Make = "ACE"
      . le16(0x0112) . le16(3) . le32(1) . le32(1)               // Orientation = 1
      . le16(0x011A) . le16(5) . le32(1) . le32(50)              // XResolution -> offset 50
      . le32(0);
$rational = le32(72) . le32(1);                                  // 72/1
$tiff = $header . $ifd0 . $rational;
$app1 = "Exif\x00\x00" . $tiff;
$jpeg = chr(0xFF).chr(0xD8).chr(0xFF).chr(0xE1).be16(strlen($app1)+2).$app1.chr(0xFF).chr(0xD9);
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6ex_");
file_put_contents($p, $jpeg);
$d = exif_read_data($p);
echo $d["Make"], "|", $d["Orientation"], "|", $d["XResolution"];
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "ACE|1|72/1");
}

/// `exif_read_data` returns false for an image with no EXIF data (array|false
/// keeps the false).
#[test]
fn test_exif_read_data_no_exif_returns_false() {
    let out = compile_and_run(
        r#"<?php
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6ne_");
$im = imagecreatetruecolor(2, 2);
imagepng($im, $p);
$d = exif_read_data($p);
echo ($d === false ? "FALSE" : "ARRAY");
"#,
    );
    assert_eq!(out, "FALSE");
}

/// `read_exif_data` is an alias of `exif_read_data` and returns the same tags.
#[test]
fn test_read_exif_data_alias() {
    let src = format!(
        r#"<?php
{HELPERS}
$header = "II" . chr(0x2A) . chr(0x00) . le32(8);
$ifd0 = le16(1)
      . le16(0x010F) . le16(2) . le32(4) . "HTC\x00"             // Make = "HTC" (fits inline)
      . le32(0);
$tiff = $header . $ifd0;
$app1 = "Exif\x00\x00" . $tiff;
$jpeg = chr(0xFF).chr(0xD8).chr(0xFF).chr(0xE1).be16(strlen($app1)+2).$app1.chr(0xFF).chr(0xD9);
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6al_");
file_put_contents($p, $jpeg);
$d = read_exif_data($p);
echo $d["Make"];
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "HTC");
}

/// `exif_thumbnail` extracts the JPEG thumbnail stored in IFD1 byte-for-byte and
/// fills the by-ref width/height/image-type out-parameters.
#[test]
fn test_exif_thumbnail() {
    let src = format!(
        r#"<?php
{HELPERS}
// A distinctive 6x9 thumbnail JPEG embedded in IFD1.
$t = imagecreatetruecolor(6, 9);
$tp = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6tj_");
imagejpeg($t, $tp);
$thumb = (string) file_get_contents($tp);
$tlen = strlen($thumb);
$header = "II" . chr(0x2A) . chr(0x00) . le32(8);
$ifd0 = le16(1) . le16(0x0112) . le16(3) . le32(1) . le32(1) . le32(26);
$ifd1 = le16(2)
      . le16(0x0201) . le16(4) . le32(1) . le32(56)              // JPEGInterchangeFormat @56
      . le16(0x0202) . le16(4) . le32(1) . le32($tlen)           // length
      . le32(0);
$tiff = $header . $ifd0 . $ifd1 . $thumb;
$app1 = "Exif\x00\x00" . $tiff;
$jpeg = chr(0xFF).chr(0xD8).chr(0xFF).chr(0xE1).be16(strlen($app1)+2).$app1.chr(0xFF).chr(0xD9);
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6wt_");
file_put_contents($p, $jpeg);
$w = 0; $h = 0; $ty = 0;
$out = exif_thumbnail($p, $w, $h, $ty);
echo strlen($out) === $tlen ? "Y" : "N";
echo "|", $w, "x", $h, "|", $ty, "|", ($out === $thumb ? "MATCH" : "DIFF");
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "Y|6x9|2|MATCH");
}

/// `exif_thumbnail` yields "" (the `string|false` collapse) when the EXIF data has
/// no thumbnail, leaving the by-ref out-params untouched.
#[test]
fn test_exif_thumbnail_none_is_empty() {
    let src = format!(
        r#"<?php
{HELPERS}
$header = "II" . chr(0x2A) . chr(0x00) . le32(8);
$ifd0 = le16(1) . le16(0x0112) . le16(3) . le32(1) . le32(1) . le32(0);
$tiff = $header . $ifd0;
$app1 = "Exif\x00\x00" . $tiff;
$jpeg = chr(0xFF).chr(0xD8).chr(0xFF).chr(0xE1).be16(strlen($app1)+2).$app1.chr(0xFF).chr(0xD9);
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6nt_");
file_put_contents($p, $jpeg);
$w = -1;
$out = exif_thumbnail($p, $w);
echo ($out === "" ? "EMPTY" : "STR:" . strlen($out)), "|", $w;
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "EMPTY|-1");
}

/// `iptcparse` decodes an IIM block into `record#dataset` keys, grouping repeated
/// datasets (keywords) into an array of values in order.
#[test]
fn test_iptcparse() {
    let src = format!(
        r#"<?php
{HELPERS}
$iptc  = chr(0x1C).chr(2).chr(5).be16(7)."caption";   // 2#005
$iptc .= chr(0x1C).chr(2).chr(25).be16(3)."kw1";       // 2#025 (first)
$iptc .= chr(0x1C).chr(2).chr(25).be16(3)."kw2";       // 2#025 (second)
$p = iptcparse($iptc);
echo $p["2#005"][0], "|", count($p["2#025"]), "|", $p["2#025"][0], ",", $p["2#025"][1];
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "caption|2|kw1,kw2");
}

/// `iptcparse` returns false when the block contains no IPTC tag marker.
#[test]
fn test_iptcparse_garbage_returns_false() {
    let out = compile_and_run(
        r#"<?php
$p = iptcparse("no iptc markers in here");
echo ($p === false ? "FALSE" : "ARRAY");
"#,
    );
    assert_eq!(out, "FALSE");
}

/// `iptcembed` inserts an IPTC block into a JPEG as a Photoshop APP13 marker; the
/// result still decodes as the original image and carries the embedded payload.
#[test]
fn test_iptcembed() {
    let src = format!(
        r#"<?php
{HELPERS}
$im = imagecreatetruecolor(5, 7);
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6em_");
imagejpeg($im, $p);
$iptc = chr(0x1C).chr(2).chr(5).be16(9)."headline!";
$embedded = iptcembed($iptc, $p);
$hasPs = strpos($embedded, "Photoshop 3.0") !== false ? "Y" : "N";
$hasData = strpos($embedded, "headline!") !== false ? "Y" : "N";
$re = imagecreatefromstring($embedded);
echo $hasPs, "|", $hasData, "|", imagesx($re), "x", imagesy($re);
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "Y|Y|5x7");
}

/// `iptcembed` replaces an existing APP13 rather than appending a second one, so a
/// re-embed leaves exactly one Photoshop marker.
#[test]
fn test_iptcembed_replaces_existing() {
    let src = format!(
        r#"<?php
{HELPERS}
$im = imagecreatetruecolor(3, 3);
$p = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6re_");
imagejpeg($im, $p);
$first = iptcembed(chr(0x1C).chr(2).chr(5).be16(3)."one", $p);
$p2 = (string) tempnam(sys_get_temp_dir(), "elephc_img_p6re2_");
file_put_contents($p2, $first);
$second = iptcembed(chr(0x1C).chr(2).chr(5).be16(3)."two", $p2);
// Count Photoshop markers by splitting on the signature.
$count = count(explode("Photoshop 3.0", $second)) - 1;
$hasOld = strpos($second, "one") !== false ? "Y" : "N";
$hasNew = strpos($second, "two") !== false ? "Y" : "N";
echo $count, "|", $hasOld, "|", $hasNew;
"#
    );
    let out = compile_and_run(&src);
    assert_eq!(out, "1|N|Y");
}
