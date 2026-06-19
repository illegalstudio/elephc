<?php

// Imagick OOP example: build an image with the object API, draw shapes with
// ImagickDraw, apply an effect, write it to disk, then read it back and inspect
// a pixel through ImagickPixel. elephc implements Imagick as a pure-Rust
// reimplementation over the same codec bridge that backs GD — no system
// ImageMagick is required.

$image = new Imagick();
$image->newImage(200, 120, "white");
$image->setImageFormat("PNG");

// Draw a filled rounded scene with ImagickDraw. Colors accept CSS names, hex,
// rgb()/rgba(), or an ImagickPixel object.
$draw = new ImagickDraw();

$draw->setFillColor("#1d4ed8");
$draw->rectangle(10, 10, 190, 60);

$draw->setFillColor(new ImagickPixel("rgb(220,38,38)"));
$draw->circle(50, 90, 50, 70);

$draw->setFillColor("gold");
$draw->setStrokeColor("black");
$draw->setStrokeWidth(2);
$draw->polygon([
    ["x" => 120, "y" => 110],
    ["x" => 150, "y" => 70],
    ["x" => 180, "y" => 110],
]);

$image->drawImage($draw);

// Soften the whole image a touch.
$image->blurImage(2, 1);

// Persist it next to this script.
$out = __DIR__ . "/scene.png";
$image->writeImage($out);
echo "Wrote " . $image->getImageWidth() . "x" . $image->getImageHeight()
    . " " . $image->getImageFormat() . " to scene.png\n";

// Read it back and report a pixel.
$loaded = new Imagick($out);
$geometry = $loaded->getImageGeometry();
echo "Reloaded geometry: " . $geometry["width"] . "x" . $geometry["height"] . "\n";

$pixel = $loaded->getImagePixelColor(30, 35);
$rgb = $pixel->getColor();
echo "Pixel (30,35): " . $pixel->getColorAsString()
    . " — r=" . $rgb["r"] . " g=" . $rgb["g"] . " b=" . $rgb["b"] . "\n";

// Build a 2-frame sequence and iterate it with the Countable / Iterator API.
$sequence = new Imagick();
$frameA = new Imagick();
$frameA->newImage(32, 32, "navy");
$frameB = new Imagick();
$frameB->newImage(64, 48, "teal");
$sequence->addImage($frameA);
$sequence->addImage($frameB);

echo "Sequence has " . count($sequence) . " frames:\n";
foreach ($sequence as $index => $frame) {
    echo "  frame " . $index . ": "
        . $frame->getImageWidth() . "x" . $frame->getImageHeight() . "\n";
}
