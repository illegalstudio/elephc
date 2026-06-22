<?php

// Cairo object API demo (pure-Rust tiny-skia bridge — no system cairo).
//
// Draws onto an image surface with paths, a transform, a stroke, and a linear
// gradient fill, writes a PNG, then reads a pixel back through GD. Run:
//   cargo run -- examples/image_cairo/main.php
//   ./examples/image_cairo/main

$surface = new CairoImageSurface(CairoFormat::ARGB32, 160, 120);
$cr = new CairoContext($surface);

// -- white background --
$cr->setSourceRgb(1, 1, 1);
$cr->paint();

// -- a filled circle, centered via a transform --
$cr->save();
$cr->translate(48, 60);
$cr->setSourceRgb(0.114, 0.306, 0.847);   // ~#1d4ed8
$cr->arc(0, 0, 34, 0, 2 * M_PI);
$cr->fill();
$cr->restore();

// -- a stroked triangle path --
$cr->setSourceRgb(0, 0.5, 0);
$cr->setLineWidth(4);
$cr->moveTo(96, 90);
$cr->lineTo(120, 40);
$cr->lineTo(144, 90);
$cr->closePath();
$cr->stroke();

// -- a horizontal bar filled with a left-to-right gradient --
$grad = new CairoLinearGradient(0, 0, 160, 0);
$grad->addColorStopRgb(0, 1, 0, 0);
$grad->addColorStopRgb(1, 0, 0, 1);
$cr->setSource($grad);
$cr->rectangle(0, 104, 160, 16);
$cr->fill();

$surface->writeToPng("cairo.png");
echo "wrote cairo.png (" . $surface->getWidth() . "x" . $surface->getHeight() . ")\n";

// -- read the circle's center pixel back through GD --
$img = imagecreatefrompng("cairo.png");
$rgb = imagecolorat($img, 48, 60);
printf("circle center = rgb(%d,%d,%d)\n", ($rgb >> 16) & 0xFF, ($rgb >> 8) & 0xFF, $rgb & 0xFF);

// -- a CairoMatrix value-object transform --
$m = new CairoMatrix();
$m->initScale(2, 3);
$p = $m->transformPoint(4, 5);
echo "scale(2,3) * (4,5) = (" . $p["x"] . "," . $p["y"] . ")\n";
