<?php

// Image transforms & filters: build a small scene, then exercise the GD
// transform/filter surface — copy, scale, crop, flip, rotate, and imagefilter —
// reading pixels back to show each step worked, and saving a few stages to PNG.
//
// Build & run:
//   cargo run -- examples/image_transform/main.php
//   ./examples/image_transform/main

$dir = sys_get_temp_dir();

// A 32x32 scene: blue background with a red square in the top-left quadrant.
$scene = imagecreatetruecolor(32, 32);
$blue = imagecolorallocate($scene, 30, 90, 200);
$red = imagecolorallocate($scene, 220, 40, 40);
imagefilledrectangle($scene, 0, 0, 31, 31, $blue);
imagefilledrectangle($scene, 0, 0, 15, 15, $red);

// --- Copy: stamp the red corner into the bottom-right quadrant ---------------
imagecopy($scene, $scene, 16, 16, 0, 0, 16, 16);
echo "copied red into bottom-right: " . imagecolorat($scene, 20, 20) . "\n";

// --- Scale: shrink the whole scene to 16x16 (bilinear) -----------------------
$small = imagescale($scene, 16, 16);
echo "scaled to " . imagesx($small) . "x" . imagesy($small) . "\n";

// --- Crop: pull out the original red quadrant --------------------------------
$crop = imagecrop($scene, ["x" => 0, "y" => 0, "width" => 16, "height" => 16]);
echo "cropped to " . imagesx($crop) . "x" . imagesy($crop)
    . ", corner = " . imagecolorat($crop, 0, 0) . "\n";

// --- Flip: mirror the scene horizontally (in place) --------------------------
imageflip($scene, IMG_FLIP_HORIZONTAL);
echo "after horizontal flip, top-left = " . imagecolorat($scene, 0, 0) . "\n";

// --- Rotate: 45 degrees counter-clockwise onto a white background ------------
$white = imagecolorallocate($scene, 255, 255, 255);
$rotated = imagerotate($scene, 45.0, $white);
echo "rotated canvas grew to " . imagesx($rotated) . "x" . imagesy($rotated) . "\n";

// --- Filters: a grayscale copy and a brightened copy -------------------------
$gray = imagecrop($scene, ["x" => 0, "y" => 0, "width" => 32, "height" => 32]);
imagefilter($gray, IMG_FILTER_GRAYSCALE);
imagefilter($gray, IMG_FILTER_BRIGHTNESS, 40);
$rgb = imagecolorsforindex($gray, imagecolorat($gray, 0, 0));
echo "filtered pixel: rgb(" . $rgb["red"] . "," . $rgb["green"] . "," . $rgb["blue"] . ")\n";

// Save a couple of stages so the output is inspectable.
imagepng($scene, $dir . "/transform_scene.png");
imagepng($rotated, $dir . "/transform_rotated.png");
imagepng($gray, $dir . "/transform_gray.png");
echo "wrote transform_scene.png, transform_rotated.png, transform_gray.png to " . $dir . "\n";

imagedestroy($small);
imagedestroy($crop);
imagedestroy($rotated);
imagedestroy($gray);
imagedestroy($scene);
