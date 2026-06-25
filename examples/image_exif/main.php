<?php

// Reading image metadata: Exif (camera/TIFF tags) and IPTC (captions/keywords).
//
// elephc parses both with a pure-Rust backend, so this runs as a standalone
// native binary with no system libexif / GD. To keep the example self-contained
// it first synthesizes a tiny EXIF-tagged JPEG in memory, then reads it back the
// same way you would read a real photo.

// --- helpers to write a minimal little-endian EXIF/TIFF APP1 segment ----------
function le16($n) { return chr($n & 0xFF) . chr(($n >> 8) & 0xFF); }
function le32($n) {
    return chr($n & 0xFF) . chr(($n >> 8) & 0xFF) . chr(($n >> 16) & 0xFF) . chr(($n >> 24) & 0xFF);
}
function be16($n) { return chr(($n >> 8) & 0xFF) . chr($n & 0xFF); }

// IFD0 with Make ("ELE"), Orientation (1 = top-left), and XResolution (72/1).
$header = "II" . chr(0x2A) . chr(0x00) . le32(8);
$ifd0 = le16(3)
      . le16(0x010F) . le16(2) . le32(4) . "ELE\x00"        // Make (ASCII, fits inline)
      . le16(0x0112) . le16(3) . le32(1) . le32(1)          // Orientation (SHORT)
      . le16(0x011A) . le16(5) . le32(1) . le32(50)         // XResolution (RATIONAL @ offset 50)
      . le32(0);
$rational = le32(72) . le32(1);
$tiff = $header . $ifd0 . $rational;
$app1 = "Exif\x00\x00" . $tiff;

// Start from a real (decodable) JPEG, then splice the EXIF APP1 segment in right
// after the SOI marker — the same shape a camera writes.
$photo = "exif_demo.jpg";
$im = imagecreatetruecolor(16, 16);
imagejpeg($im, $photo);
$base = (string) file_get_contents($photo);
$exifSegment = chr(0xFF) . chr(0xE1) . be16(strlen($app1) + 2) . $app1;
file_put_contents($photo, substr($base, 0, 2) . $exifSegment . substr($base, 2));

// --- exif_imagetype: a quick header sniff -------------------------------------
$type = (int) exif_imagetype($photo);
echo "File type code: ", $type, " (", image_type_to_mime_type($type), ")\n";

// --- exif_read_data: the full tag set, keyed by EXIF mnemonic -----------------
$exif = exif_read_data($photo);
if ($exif === false) {
    echo "No EXIF data found.\n";
} else {
    echo "Make:        ", $exif["Make"], "\n";
    echo "Orientation: ", $exif["Orientation"], "\n";
    echo "XResolution: ", $exif["XResolution"], "\n";
}

// --- exif_tagname: look up a tag's name by its number -------------------------
echo "Tag 0x0112 is ", exif_tagname(0x0112), "\n";

// --- IPTC: build a caption + keywords block, then read it back ----------------
$iptc  = chr(0x1C) . chr(2) . chr(5)  . be16(11) . "Sunset over";    // 2#005 caption
$iptc .= chr(0x1C) . chr(2) . chr(25) . be16(6)  . "sunset";         // 2#025 keyword
$iptc .= chr(0x1C) . chr(2) . chr(25) . be16(5)  . "beach";          // 2#025 keyword

$parsed = iptcparse($iptc);
echo "Caption:  ", $parsed["2#005"][0], "\n";
echo "Keywords: ", implode(", ", $parsed["2#025"]), "\n";

// Embed the IPTC block into the JPEG as a Photoshop APP13 marker.
$withIptc = iptcembed($iptc, $photo);
echo "JPEG with IPTC is ", strlen($withIptc), " bytes\n";
