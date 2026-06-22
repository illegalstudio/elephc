<?php

// Cairo procedural API demo (pure-Rust tiny-skia bridge — no system cairo).
//
// Same scene as examples/image_cairo/main.php, but driven through the PECL-style
// free-function layer (cairo_create, cairo_set_source_*, cairo_arc, ...) instead of
// the CairoContext object. It writes a PNG, reads a pixel back through GD, and shows
// the matrix helpers. Run:
//   cargo run -- examples/image_cairo_procedural/main.php
//   ./examples/image_cairo_procedural/main

$surface = cairo_image_surface_create(CairoFormat::ARGB32, 160, 120);
$cr = cairo_create($surface);

// -- white background --
cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);

// -- a filled circle, centered via a transform --
cairo_save($cr);
cairo_translate($cr, 48, 60);
cairo_set_source_rgb($cr, 0.114, 0.306, 0.847);   // ~#1d4ed8
cairo_arc($cr, 0, 0, 34, 0, 2 * M_PI);
cairo_fill($cr);
cairo_restore($cr);

// -- a stroked triangle path --
cairo_set_source_rgb($cr, 0, 0.5, 0);
cairo_set_line_width($cr, 4);
cairo_move_to($cr, 96, 90);
cairo_line_to($cr, 120, 40);
cairo_line_to($cr, 144, 90);
cairo_close_path($cr);
cairo_stroke($cr);

// -- a horizontal bar filled with a left-to-right gradient --
$grad = cairo_pattern_create_linear(0, 0, 160, 0);
cairo_pattern_add_color_stop_rgb($grad, 0, 1, 0, 0);
cairo_pattern_add_color_stop_rgb($grad, 1, 0, 0, 1);
cairo_set_source($cr, $grad);
cairo_rectangle($cr, 0, 104, 160, 16);
cairo_fill($cr);

cairo_surface_write_to_png($surface, "cairo_proc.png");
echo "wrote cairo_proc.png (" . cairo_image_surface_get_width($surface) . "x"
    . cairo_image_surface_get_height($surface) . ")\n";

// -- read the circle's center pixel back through GD --
$img = imagecreatefrompng("cairo_proc.png");
$rgb = imagecolorat($img, 48, 60);
printf("circle center = rgb(%d,%d,%d)\n", ($rgb >> 16) & 0xFF, ($rgb >> 8) & 0xFF, $rgb & 0xFF);

// -- load the PNG we just wrote back into a cairo surface (decode round-trip) --
$loaded = cairo_image_surface_create_from_png("cairo_proc.png");
echo "reloaded " . cairo_image_surface_get_width($loaded) . "x"
    . cairo_image_surface_get_height($loaded) . "\n";

// -- matrix helpers: init_scale + transform_point, and a composed multiply --
$m = cairo_matrix_init_scale(2, 3);
$p = cairo_matrix_transform_point($m, 4, 5);
echo "scale(2,3) * (4,5) = (" . $p["x"] . "," . $p["y"] . ")\n";

$prod = cairo_matrix_multiply(cairo_matrix_init_scale(2, 3), cairo_matrix_init_translate(5, 7));
$q = cairo_matrix_transform_point($prod, 1, 1);
echo "scale(2,3) ∘ translate(5,7) * (1,1) = (" . $q["x"] . "," . $q["y"] . ")\n";