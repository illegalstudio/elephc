<?php

// Image basics: create a true-color image with GD, draw shapes on a blue
// background, save it across several formats (PNG/JPEG/WebP), read the size back
// with getimagesize(), decode an image back from an in-memory string, probe a
// string with getimagesizefromstring(), and read a TGA file with
// imagecreatefromtga().
//
// Build & run:
//   cargo run -- examples/image_basics/main.php
//   ./examples/image_basics/main

$width = 64;
$height = 48;

$im = imagecreatetruecolor($width, $height);

$blue = imagecolorallocate($im, 30, 90, 200);
$red = imagecolorallocate($im, 220, 40, 40);
$yellow = imagecolorallocate($im, 240, 210, 40);

// Fill the background blue, draw a red diagonal, outline a rectangle, and drop a
// filled yellow ellipse in the middle.
imagefilledrectangle($im, 0, 0, $width - 1, $height - 1, $blue);
imageline($im, 0, 0, $width - 1, $height - 1, $red);
imagerectangle($im, 4, 4, $width - 5, $height - 5, $red);
$cx = intdiv($width, 2);
$cy = intdiv($height, 2);
imagefilledellipse($im, $cx, $cy, 24, 18, $yellow);

// Label it with the built-in bitmap font.
$white = imagecolorallocate($im, 255, 255, 255);
imagestring($im, 3, 3, 2, "elephc", $white);

echo "created " . imagesx($im) . "x" . imagesy($im)
    . (imageistruecolor($im) ? " true-color" : " palette") . " image\n";

// Read the center pixel back and unpack its components (it sits in the ellipse).
$rgb = imagecolorsforindex($im, imagecolorat($im, $cx, $cy));
echo "center pixel: rgb(" . $rgb["red"] . "," . $rgb["green"] . "," . $rgb["blue"] . ")\n";

$dir = sys_get_temp_dir();
$png = $dir . "/image_basics.png";
$jpg = $dir . "/image_basics.jpg";
$webp = $dir . "/image_basics.webp";

// The same image can be written to any supported format.
imagepng($im, $png);
imagejpeg($im, $jpg, 90);
imagewebp($im, $webp);

foreach ([$png, $jpg, $webp] as $path) {
    $info = getimagesize($path);
    // getimagesize() returns a mixed array, so cast the type code to int before
    // passing it to a function with an `int` parameter.
    echo "wrote " . $info[0] . "x" . $info[1]
        . " (" . $info["mime"] . ", "
        . image_type_to_extension((int) $info[2], false) . ")\n";
}

// Decode an image straight from a string of bytes (here, the PNG we just wrote).
$bytes = (string) file_get_contents($png);
$decoded = imagecreatefromstring($bytes);
echo "decoded from string: " . imagesx($decoded) . "x" . imagesy($decoded) . "\n";

// getimagesizefromstring() probes those same bytes without a file path, returning
// the same shape as getimagesize(). As with getimagesize(), the mixed array needs
// a cast before a typed call.
$sinfo = getimagesizefromstring($bytes);
echo "probed from string: " . $sinfo[0] . "x" . $sinfo[1] . " (" . $sinfo["mime"] . ")\n";

// TGA is a read-only format in PHP since 7.4. A TGA header carries no sniffable
// magic, so imagecreatefromtga() pins the format itself. Build a tiny 2x2
// uncompressed true-color (24-bit BGR) TGA, write it, and read it back.
function le16($n) { return chr($n & 0xFF) . chr(($n >> 8) & 0xFF); }
$tga_header = chr(0) . chr(0) . chr(2)
    . le16(0) . le16(0) . chr(0)
    . le16(0) . le16(0)
    . le16(2) . le16(2)
    . chr(24) . chr(0);
$tga_px = chr(0x00) . chr(0x00) . chr(0xFF);   // one BGR pixel = red
$tga_bytes = $tga_header . str_repeat($tga_px, 4);
$tga_path = $dir . "/image_basics.tga";
file_put_contents($tga_path, $tga_bytes);
$tga_im = imagecreatefromtga($tga_path);
echo "decoded tga: " . imagesx($tga_im) . "x" . imagesy($tga_im)
    . " px=" . imagecolorat($tga_im, 0, 0) . "\n";

imagedestroy($decoded);
imagedestroy($tga_im);
imagedestroy($im);
unlink($tga_path);
