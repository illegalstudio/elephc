<?php

// Gmagick object API demo (pure-Rust bridge — no system GraphicsMagick).
//
// Builds a small banner with GmagickDraw, applies a fluent transform chain, writes
// a PNG, reads it back, and assembles a two-frame wand. Run:
//   cargo run -- examples/image_gmagick/main.php
//   ./examples/image_gmagick/main

// -- create a canvas and draw on it (GmagickDraw is fluent) --
$gm = new Gmagick();
$gm->newImage(120, 60, "rgb(245,245,245)", "PNG");

$draw = new GmagickDraw();
$draw->setFillColor("#1d4ed8")
     ->rectangle(8, 8, 60, 50);
$draw->setFillColor(new GmagickPixel("rgb(0,128,0)"))
     ->polygon([
         ["x" => 70, "y" => 50],
         ["x" => 90, "y" => 10],
         ["x" => 110, "y" => 50],
     ]);
$gm->drawImage($draw);

// -- fluent transform chain, then write --
$gm->setCompressionQuality(90)
   ->scaleImage(240, 120)
   ->writeImage("banner.png");

echo "wrote banner.png (" . $gm->getImageWidth() . "x" . $gm->getImageHeight() . ", "
    . $gm->getImageFormat() . ")\n";

// -- read it back and inspect a pixel via GD (Gmagick has no pixel-read API) --
$back = new Gmagick();
$back->readImage("banner.png");
$img = imagecreatefromstring($back->getImageBlob());
$rgb = imagecolorat($img, 40, 40);
printf("pixel(40,40) = rgb(%d,%d,%d)\n", ($rgb >> 16) & 0xFF, ($rgb >> 8) & 0xFF, $rgb & 0xFF);

// -- inspect a GmagickPixel --
$px = new GmagickPixel("#1d4ed8");
$c = $px->getColor();
echo "fill " . $px->getColorAsString() . " => r=" . $c["r"] . " g=" . $c["g"] . " b=" . $c["b"] . "\n";

// -- a two-frame wand --
$frames = new Gmagick();
$frames->newImage(16, 16, "red");
$second = new Gmagick();
$second->newImage(16, 16, "blue");
$frames->addImage($second);
echo "frames: " . $frames->getNumberImages() . "\n";
