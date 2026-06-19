//! Purpose:
//! PHP image standard-library surface (GD, Exif/IPTC, Imagick, Gmagick, Cairo),
//! implemented in elephc-PHP on top of the pure-Rust `elephc_image` bridge.
//! Declares the `elephc_image` externs, the `IMAGETYPE_*` constants, the
//! `GdImage` object, and the procedural image functions, so the feature compiles
//! through the normal pipeline (functions, classes, destructors, C-ABI extern
//! calls) with no codegen intrinsics.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via
//!   `inject_if_used`, after include resolution and before name resolution.
//!
//! Key details:
//! - The prelude is injected only when the program references an image symbol
//!   (see `detect`), so non-image binaries never declare `elephc_image` externs
//!   and never link `-lelephc_image`.
//! - `GdImage` holds the bridge's opaque `int` handle and frees it in
//!   `__destruct`; `imagedestroy()` frees it explicitly. The bridge's destroy is
//!   idempotent, so the two paths cannot double-free.
//! - Implements the full PHP image surface: the always-available core
//!   (`getimagesize`, `image_type_to_mime_type`, `image_type_to_extension`); GD
//!   raster I/O for PNG/JPEG/GIF/BMP/WebP (`imagecreatefrom*`,
//!   `imagecreatefromstring`, and the `image{png,jpeg,gif,bmp,webp}` output
//!   family, file or in-memory/stdout); the GD info functions
//!   (`imageistruecolor`, `imageresolution`, `imagetypes`, `gd_info`); GD color,
//!   drawing, text, transforms, and filters; Exif/IPTC metadata; and the Imagick,
//!   Gmagick, and Cairo OOP surfaces plus the procedural `cairo_*` API. Binary
//!   blobs cross the boundary through the bridge's staging buffer / encode cell
//!   plus `ptr_write_string` / `ptr_read_string`, since extern `string` is
//!   NUL-terminated and cannot carry encoded image bytes.

mod detect;

/// The elephc-PHP image prelude: `elephc_image` externs, `IMAGETYPE_*`
/// constants, the `GdImage` class, and the procedural image functions.
const IMAGE_PRELUDE_SRC: &str = r#"<?php

extern "elephc_image" {
    function elephc_img_create_truecolor(int $width, int $height): int;
    function elephc_img_create(int $width, int $height): int;
    function elephc_img_color_allocate(int $handle, int $red, int $green, int $blue): int;
    function elephc_img_color_allocate_alpha(int $handle, int $red, int $green, int $blue, int $alpha): int;
    function elephc_img_set_pixel(int $handle, int $x, int $y, int $color): void;
    function elephc_img_sx(int $handle): int;
    function elephc_img_sy(int $handle): int;
    function elephc_img_is_truecolor(int $handle): int;
    function elephc_img_res_x(int $handle): int;
    function elephc_img_res_y(int $handle): int;
    function elephc_img_set_res(int $handle, int $res_x, int $res_y): void;
    function elephc_img_color_at(int $handle, int $x, int $y): int;
    function elephc_img_set_alpha_blending(int $handle, int $on): void;
    function elephc_img_set_save_alpha(int $handle, int $on): void;
    function elephc_img_set_transparent(int $handle, int $color): void;
    function elephc_img_get_transparent(int $handle): int;
    function elephc_img_color_total(int $handle): int;
    function elephc_img_set_truecolor(int $handle, int $on): void;
    function elephc_img_set_thickness(int $handle, int $thickness): void;
    function elephc_img_line(int $handle, int $x1, int $y1, int $x2, int $y2, int $color): void;
    function elephc_img_dashed_line(int $handle, int $x1, int $y1, int $x2, int $y2, int $color): void;
    function elephc_img_rectangle(int $handle, int $x1, int $y1, int $x2, int $y2, int $color): void;
    function elephc_img_filled_rectangle(int $handle, int $x1, int $y1, int $x2, int $y2, int $color): void;
    function elephc_img_ellipse(int $handle, int $cx, int $cy, int $w, int $h, int $color): void;
    function elephc_img_filled_ellipse(int $handle, int $cx, int $cy, int $w, int $h, int $color): void;
    function elephc_img_arc(int $handle, int $cxy, int $wh, int $start, int $end, int $color): void;
    function elephc_img_filled_arc(int $handle, int $cxy, int $wh, int $start, int $end, int $color): void;
    function elephc_img_fill(int $handle, int $x, int $y, int $color): void;
    function elephc_img_fill_to_border(int $handle, int $x, int $y, int $border, int $color): void;
    function elephc_img_poly_reset(): void;
    function elephc_img_poly_add(int $x, int $y): void;
    function elephc_img_poly_line(int $handle, int $color, int $closed): void;
    function elephc_img_poly_fill(int $handle, int $color): void;
    function elephc_img_string(int $handle, int $font, int $x, int $y, int $color, string $text): void;
    function elephc_img_string_up(int $handle, int $font, int $x, int $y, int $color, string $text): void;
    function elephc_img_destroy(int $handle): void;
    function elephc_img_stage_ptr(int $len): ptr;
    function elephc_img_create_from_stage(int $len): int;
    function elephc_img_create_from_file(string $path, int $expected_fmt): int;
    function elephc_img_write_file(int $handle, int $fmt, string $path, int $quality): int;
    function elephc_img_encode(int $handle, int $fmt, int $quality): int;
    function elephc_img_encoded_ptr(): ptr;
    function elephc_img_encoded_len(): int;
    function elephc_img_encoded_clear(): void;
    function elephc_img_probe_file(string $path): int;
    function elephc_img_probe_stage(int $len): int;
    function elephc_img_probe_width(): int;
    function elephc_img_probe_height(): int;
    function elephc_img_probe_type(): int;
    function elephc_img_probe_bits(): int;
    function elephc_img_probe_channels(): int;
    function elephc_img_fbuf_reset(): void;
    function elephc_img_fbuf_push(int $fixed16): void;
    function elephc_img_copy(int $dst, int $src, int $dxy, int $sxy, int $swh): int;
    function elephc_img_copy_merge(int $dst, int $src, int $dxy, int $sxy, int $swh, int $pct): int;
    function elephc_img_copy_merge_gray(int $dst, int $src, int $dxy, int $sxy, int $swh, int $pct): int;
    function elephc_img_copy_resized(int $dst, int $src, int $dxy, int $sxy, int $dwh, int $swh): int;
    function elephc_img_copy_resampled(int $dst, int $src, int $dxy, int $sxy, int $dwh, int $swh): int;
    function elephc_img_scale(int $src, int $new_w, int $new_h, int $mode): int;
    function elephc_img_crop(int $src, int $x, int $y, int $w, int $h): int;
    function elephc_img_crop_auto(int $src, int $mode, int $color, int $threshold_permille): int;
    function elephc_img_flip(int $handle, int $mode): int;
    function elephc_img_rotate(int $src, int $angle_mdeg, int $bgcolor): int;
    function elephc_img_affine(int $src): int;
    function elephc_img_filter(int $handle, int $filter, int $a1, int $a2, int $a3, int $a4): int;
    function elephc_img_convolution(int $handle, int $div_fixed, int $offset_fixed): int;
    function elephc_img_gamma(int $handle, int $in_fixed, int $out_fixed): int;
    function elephc_img_set_interpolation(int $handle, int $method): void;
    function elephc_img_get_interpolation(int $handle): int;
    function elephc_img_set_interlace(int $handle, int $on): void;
    function elephc_img_get_interlace(int $handle): int;
    function elephc_img_in_ptr(int $len): ptr;
    function elephc_img_out_ptr(): ptr;
    function elephc_img_kv_count(): int;
    function elephc_img_kv_key(int $index): int;
    function elephc_img_kv_val(int $index): int;
    function elephc_exif_read(string $path): int;
    function elephc_exif_tagname(int $number): int;
    function elephc_exif_thumbnail(string $path): int;
    function elephc_exif_thumb_width(): int;
    function elephc_exif_thumb_height(): int;
    function elephc_exif_thumb_type(): int;
    function elephc_iptc_parse(int $len): int;
    function elephc_iptc_key_count(): int;
    function elephc_iptc_key(int $index): int;
    function elephc_iptc_val_count(int $index): int;
    function elephc_iptc_val(int $key_index, int $val_index): int;
    function elephc_iptc_embed(string $path, int $in_len): int;
    // -- Imagick wand lifecycle / I/O / iteration --
    function elephc_imagick_new(): int;
    function elephc_imagick_destroy(int $wand): void;
    function elephc_imagick_clear(int $wand): void;
    function elephc_imagick_count(int $wand): int;
    function elephc_imagick_read_file(int $wand, string $path): int;
    function elephc_imagick_read_blob(int $wand, int $len): int;
    function elephc_imagick_new_image(int $wand, int $w, int $h, int $bg, int $fmt): int;
    function elephc_imagick_add_image(int $dst, int $src): int;
    function elephc_imagick_cur_width(int $wand): int;
    function elephc_imagick_cur_height(int $wand): int;
    function elephc_imagick_set_format(int $wand, int $fmt): void;
    function elephc_imagick_get_format(int $wand): int;
    function elephc_imagick_set_quality(int $wand, int $quality): void;
    function elephc_imagick_get_quality(int $wand): int;
    function elephc_imagick_write_file(int $wand, string $path, int $fmt_override): int;
    function elephc_imagick_get_blob(int $wand, int $fmt_override): int;
    function elephc_imagick_get_index(int $wand): int;
    function elephc_imagick_set_index(int $wand, int $index): int;
    function elephc_imagick_next(int $wand): int;
    function elephc_imagick_previous(int $wand): int;
    function elephc_imagick_first(int $wand): void;
    function elephc_imagick_last(int $wand): void;
    function elephc_imagick_pixel_color(int $wand, int $x, int $y): int;
    function elephc_imagick_fill(int $wand, int $color): int;
    // -- Imagick transforms / effects / compositing --
    function elephc_imagick_resize(int $wand, int $cols, int $rows): int;
    function elephc_imagick_scale(int $wand, int $cols, int $rows): int;
    function elephc_imagick_crop(int $wand, int $w, int $h, int $x, int $y): int;
    function elephc_imagick_rotate(int $wand, int $angle_mdeg, int $bg): int;
    function elephc_imagick_flip(int $wand): int;
    function elephc_imagick_flop(int $wand): int;
    function elephc_imagick_blur(int $wand, int $sigma_milli): int;
    function elephc_imagick_negate(int $wand, int $only_gray): int;
    function elephc_imagick_modulate(int $wand, int $b, int $s, int $h): int;
    function elephc_imagick_sharpen(int $wand, int $radius_milli, int $sigma_milli): int;
    function elephc_imagick_composite(int $dst, int $src, int $op, int $x, int $y): int;
    function elephc_imagick_convolve(int $wand, int $div_fixed, int $offset_fixed): int;
    // -- ImagickDraw command buffer + render --
    function elephc_idraw_new(): int;
    function elephc_idraw_destroy(int $draw): void;
    function elephc_idraw_clear(int $draw): void;
    function elephc_idraw_set_fill(int $draw, int $color): void;
    function elephc_idraw_set_stroke(int $draw, int $color): void;
    function elephc_idraw_set_stroke_width(int $draw, int $width): void;
    function elephc_idraw_get_fill(int $draw): int;
    function elephc_idraw_line(int $draw, int $x1, int $y1, int $x2, int $y2): void;
    function elephc_idraw_rectangle(int $draw, int $x1, int $y1, int $x2, int $y2): void;
    function elephc_idraw_circle(int $draw, int $ox, int $oy, int $px, int $py): void;
    function elephc_idraw_ellipse(int $draw, int $oxy, int $rxy, int $se): void;
    function elephc_idraw_point(int $draw, int $x, int $y): void;
    function elephc_idraw_poly_reset(int $draw): void;
    function elephc_idraw_poly_point(int $draw, int $x, int $y): void;
    function elephc_idraw_polygon(int $draw): void;
    function elephc_imagick_draw(int $wand, int $draw): int;

    // -- Cairo (tiny-skia) bridge --
    function elephc_cairo_surface_create(int $w, int $h): int;
    function elephc_cairo_surface_destroy(int $s): void;
    function elephc_cairo_surface_width(int $s): int;
    function elephc_cairo_surface_height(int $s): int;
    function elephc_cairo_surface_encode_png(int $s): int;
    function elephc_cairo_surface_write_png(int $s, string $path): int;
    function elephc_cairo_surface_create_from_png(string $path): int;
    function elephc_cairo_create(int $surface): int;
    function elephc_cairo_destroy(int $ctx): void;
    function elephc_cairo_save(int $ctx): void;
    function elephc_cairo_restore(int $ctx): void;
    function elephc_cairo_set_source_rgba(int $ctx, int $packed): void;
    function elephc_cairo_set_source_pattern(int $ctx, int $pattern): void;
    function elephc_cairo_set_line_width(int $ctx, int $w): void;
    function elephc_cairo_set_line_cap(int $ctx, int $cap): void;
    function elephc_cairo_set_line_join(int $ctx, int $join): void;
    function elephc_cairo_set_fill_rule(int $ctx, int $rule): void;
    function elephc_cairo_move_to(int $ctx, int $p_xy): void;
    function elephc_cairo_line_to(int $ctx, int $p_xy): void;
    function elephc_cairo_curve_to(int $ctx, int $p1, int $p2, int $p3): void;
    function elephc_cairo_rectangle(int $ctx, int $p_xy, int $p_wh): void;
    function elephc_cairo_arc(int $ctx, int $p_center, int $radius_fx, int $p_angles): void;
    function elephc_cairo_arc_negative(int $ctx, int $p_center, int $radius_fx, int $p_angles): void;
    function elephc_cairo_close_path(int $ctx): void;
    function elephc_cairo_new_path(int $ctx): void;
    function elephc_cairo_new_sub_path(int $ctx): void;
    function elephc_cairo_translate(int $ctx, int $p_xy): void;
    function elephc_cairo_scale(int $ctx, int $p_sxsy): void;
    function elephc_cairo_rotate(int $ctx, int $angle_mrad): void;
    function elephc_cairo_set_matrix(int $ctx, int $p_ab, int $p_cd, int $p_ef): void;
    function elephc_cairo_transform(int $ctx, int $p_ab, int $p_cd, int $p_ef): void;
    function elephc_cairo_identity_matrix(int $ctx): void;
    function elephc_cairo_get_current_point_x(int $ctx): int;
    function elephc_cairo_get_current_point_y(int $ctx): int;
    function elephc_cairo_paint(int $ctx): void;
    function elephc_cairo_fill(int $ctx): void;
    function elephc_cairo_fill_preserve(int $ctx): void;
    function elephc_cairo_stroke(int $ctx): void;
    function elephc_cairo_stroke_preserve(int $ctx): void;
    function elephc_cairo_pattern_create_rgba(int $packed): int;
    function elephc_cairo_pattern_create_linear(int $p0, int $p1): int;
    function elephc_cairo_pattern_create_radial(int $p_c0, int $r0_fx, int $p_c1, int $r1_fx): int;
    function elephc_cairo_pattern_add_color_stop_rgba(int $pattern, int $offset_fx, int $packed): void;
    function elephc_cairo_pattern_destroy(int $pattern): void;
}

const IMAGETYPE_UNKNOWN = 0;
const IMAGETYPE_GIF = 1;
const IMAGETYPE_JPEG = 2;
const IMAGETYPE_PNG = 3;
const IMAGETYPE_SWF = 4;
const IMAGETYPE_PSD = 5;
const IMAGETYPE_BMP = 6;
const IMAGETYPE_TIFF_II = 7;
const IMAGETYPE_TIFF_MM = 8;
const IMAGETYPE_JPC = 9;
const IMAGETYPE_JP2 = 10;
const IMAGETYPE_JPX = 11;
const IMAGETYPE_JB2 = 12;
const IMAGETYPE_SWC = 13;
const IMAGETYPE_IFF = 14;
const IMAGETYPE_WBMP = 15;
const IMAGETYPE_XBM = 16;
const IMAGETYPE_ICO = 17;
const IMAGETYPE_WEBP = 18;
const IMAGETYPE_AVIF = 19;
const IMAGETYPE_COUNT = 20;

// Image-type bitmask values for imagetypes()/gd_info(), matching PHP. IMG_JPG and
// IMG_JPEG are aliases (both 2), as in PHP.
const IMG_GIF = 1;
const IMG_JPG = 2;
const IMG_JPEG = 2;
const IMG_PNG = 4;
const IMG_WBMP = 8;
const IMG_XPM = 16;
const IMG_WEBP = 32;
const IMG_BMP = 64;
const IMG_TGA = 128;
const IMG_AVIF = 256;

// Layer-effect modes for imagelayereffect(), matching PHP.
const IMG_EFFECT_REPLACE = 0;
const IMG_EFFECT_ALPHABLEND = 1;
const IMG_EFFECT_NORMAL = 2;
const IMG_EFFECT_OVERLAY = 3;
const IMG_EFFECT_MULTIPLY = 4;

// Arc style flags for imagefilledarc(), matching PHP (combinable bitmask).
const IMG_ARC_PIE = 0;
const IMG_ARC_CHORD = 1;
const IMG_ARC_NOFILL = 2;
const IMG_ARC_EDGED = 4;

// Flip modes for imageflip(), matching PHP.
const IMG_FLIP_HORIZONTAL = 1;
const IMG_FLIP_VERTICAL = 2;
const IMG_FLIP_BOTH = 3;

// imagefilter() selectors, matching PHP.
const IMG_FILTER_NEGATE = 0;
const IMG_FILTER_GRAYSCALE = 1;
const IMG_FILTER_BRIGHTNESS = 2;
const IMG_FILTER_CONTRAST = 3;
const IMG_FILTER_COLORIZE = 4;
const IMG_FILTER_EDGEDETECT = 5;
const IMG_FILTER_EMBOSS = 6;
const IMG_FILTER_GAUSSIAN_BLUR = 7;
const IMG_FILTER_SELECTIVE_BLUR = 8;
const IMG_FILTER_MEAN_REMOVAL = 9;
const IMG_FILTER_SMOOTH = 10;
const IMG_FILTER_PIXELATE = 11;
const IMG_FILTER_SCATTER = 12;

// Affine matrix element selectors for imageaffinematrixget(), matching PHP.
const IMG_AFFINE_TRANSLATE = 0;
const IMG_AFFINE_SCALE = 1;
const IMG_AFFINE_ROTATE = 2;
const IMG_AFFINE_SHEAR_HORIZONTAL = 3;
const IMG_AFFINE_SHEAR_VERTICAL = 4;

// imagecropauto() modes, matching PHP.
const IMG_CROP_DEFAULT = 0;
const IMG_CROP_TRANSPARENT = 1;
const IMG_CROP_BLACK = 2;
const IMG_CROP_WHITE = 3;
const IMG_CROP_SIDES = 4;
const IMG_CROP_THRESHOLD = 5;

// Pixel interpolation methods for imagesetinterpolation(), matching PHP's GD
// gdInterpolationMethod enum. IMG_BILINEAR_FIXED (3) is GD's default.
const IMG_BELL = 1;
const IMG_BESSEL = 2;
const IMG_BILINEAR_FIXED = 3;
const IMG_BICUBIC = 4;
const IMG_BICUBIC_FIXED = 5;
const IMG_BLACKMAN = 6;
const IMG_BOX = 7;
const IMG_BSPLINE = 8;
const IMG_CATMULLROM = 9;
const IMG_GAUSSIAN = 10;
const IMG_GENERALIZED_CUBIC = 11;
const IMG_HERMITE = 12;
const IMG_HAMMING = 13;
const IMG_HANNING = 14;
const IMG_MITCHELL = 15;
const IMG_NEAREST_NEIGHBOUR = 16;
const IMG_POWER = 17;
const IMG_QUADRATIC = 18;
const IMG_SINC = 19;
const IMG_TRIANGLE = 20;
const IMG_WEIGHTED4 = 21;

final class GdImage {
    public int $handle = 0;

    public function __construct(int $handle) {
        $this->handle = $handle;
    }

    public function __destruct() {
        elephc_img_destroy($this->handle);
    }
}

function imagecreatetruecolor(int $width, int $height): GdImage {
    $handle = elephc_img_create_truecolor($width, $height);
    return new GdImage($handle);
}

function imagecreate(int $width, int $height): GdImage {
    $handle = elephc_img_create($width, $height);
    return new GdImage($handle);
}

function imagecolorallocate(GdImage $image, int $red, int $green, int $blue): int {
    return elephc_img_color_allocate($image->handle, $red, $green, $blue);
}

function imagecolorallocatealpha(GdImage $image, int $red, int $green, int $blue, int $alpha): int {
    return elephc_img_color_allocate_alpha($image->handle, $red, $green, $blue, $alpha);
}

function imagesetpixel(GdImage $image, int $x, int $y, int $color): bool {
    elephc_img_set_pixel($image->handle, $x, $y, $color);
    return true;
}

function imagesx(GdImage $image): int {
    return elephc_img_sx($image->handle);
}

function imagesy(GdImage $image): int {
    return elephc_img_sy($image->handle);
}

function imagedestroy(GdImage $image): bool {
    elephc_img_destroy($image->handle);
    return true;
}

function imageistruecolor(GdImage $image): bool {
    return elephc_img_is_truecolor($image->handle) === 1;
}

function imageresolution(GdImage $image, ?int $resolution_x = null, ?int $resolution_y = null): array|bool {
    if ($resolution_x === null) {
        return [elephc_img_res_x($image->handle), elephc_img_res_y($image->handle)];
    }
    // A single argument sets both axes to the same value, matching GD. The casts
    // pin the nullable params to int for the extern (the null case returned
    // above, but the checker does not narrow `?int` after the guard).
    $_ry = $resolution_y ?? $resolution_x;
    elephc_img_set_res($image->handle, (int) $resolution_x, (int) $_ry);
    return true;
}

// ---- Color handling -------------------------------------------------------
// Every elephc image is true-color RGBA, so the palette-oriented "closest"/
// "exact"/"resolve" lookups all reduce to packing the requested RGB(A) into a GD
// color value (there is no indexed palette to search). Functions that only read
// or repack a color still take the GdImage for PHP signature compatibility.

function imagecolorat(GdImage $image, int $x, int $y): int {
    return elephc_img_color_at($image->handle, $x, $y);
}

function imagecolorsforindex(GdImage $image, int $color): array {
    $_unused = $image;
    // Unpack a GD packed color; alpha is GD's 7-bit value (0 opaque … 127 clear).
    return [
        "red" => ($color >> 16) & 0xFF,
        "green" => ($color >> 8) & 0xFF,
        "blue" => $color & 0xFF,
        "alpha" => ($color >> 24) & 0x7F,
    ];
}

function imagecolordeallocate(GdImage $image, int $color): bool {
    // True-color images have no palette slots to free; this is a no-op success.
    $_unused = $image;
    $_color = $color;
    return true;
}

function imagecolorexact(GdImage $image, int $red, int $green, int $blue): int {
    return elephc_img_color_allocate($image->handle, $red, $green, $blue);
}

function imagecolorexactalpha(GdImage $image, int $red, int $green, int $blue, int $alpha): int {
    return elephc_img_color_allocate_alpha($image->handle, $red, $green, $blue, $alpha);
}

function imagecolorclosest(GdImage $image, int $red, int $green, int $blue): int {
    return elephc_img_color_allocate($image->handle, $red, $green, $blue);
}

function imagecolorclosestalpha(GdImage $image, int $red, int $green, int $blue, int $alpha): int {
    return elephc_img_color_allocate_alpha($image->handle, $red, $green, $blue, $alpha);
}

function imagecolorclosesthwb(GdImage $image, int $red, int $green, int $blue): int {
    return elephc_img_color_allocate($image->handle, $red, $green, $blue);
}

function imagecolorresolve(GdImage $image, int $red, int $green, int $blue): int {
    return elephc_img_color_allocate($image->handle, $red, $green, $blue);
}

function imagecolorresolvealpha(GdImage $image, int $red, int $green, int $blue, int $alpha): int {
    return elephc_img_color_allocate_alpha($image->handle, $red, $green, $blue, $alpha);
}

function imagecolortransparent(GdImage $image, ?int $color = null): int {
    if ($color === null) {
        return elephc_img_get_transparent($image->handle);
    }
    $_c = (int) $color;
    elephc_img_set_transparent($image->handle, $_c);
    return $_c;
}

function imagecolorstotal(GdImage $image): int {
    return elephc_img_color_total($image->handle);
}

function imagealphablending(GdImage $image, bool $enable): bool {
    elephc_img_set_alpha_blending($image->handle, $enable ? 1 : 0);
    return true;
}

function imagesavealpha(GdImage $image, bool $enable): bool {
    elephc_img_set_save_alpha($image->handle, $enable ? 1 : 0);
    return true;
}

function imagepalettetotruecolor(GdImage $image): bool {
    elephc_img_set_truecolor($image->handle, 1);
    return true;
}

function imagetruecolortopalette(GdImage $image, bool $dither, int $num_colors): bool {
    // elephc stores every image as RGBA, so this flips the true-color flag
    // without an actual quantization pass; $dither/$num_colors are ignored.
    $_unused = $dither;
    $_n = $num_colors;
    elephc_img_set_truecolor($image->handle, 0);
    return true;
}

function imagecolormatch(GdImage $image1, GdImage $image2): bool {
    // Palette↔true-color color matching is a no-op here (no real palette model).
    $_u1 = $image1;
    $_u2 = $image2;
    return true;
}

// imagecolorset / imagepalettecopy: palette-entry operations. elephc stores
// every image as a true-color RGBA buffer (see docs/php/image.md), so there are
// no palette slots to recolor or copy. They are accepted as no-op successes for
// API completeness; code that depends on a palette color actually changing after
// imagecolorset should treat that as a documented limitation.
function imagecolorset(GdImage $image, int $color, int $red, int $green, int $blue, int $alpha = 0): bool {
    // No palette slots to recolor (every image is RGBA); no-op success.
    $_image = $image;
    $_color = $color;
    $_red = $red;
    $_green = $green;
    $_blue = $blue;
    $_alpha = $alpha;
    return true;
}

function imagepalettecopy(GdImage $dst, GdImage $src): bool {
    // No palette to copy (every image is RGBA); no-op success.
    $_dst = $dst;
    $_src = $src;
    return true;
}

function imagelayereffect(GdImage $image, int $effect): bool {
    // Map GD's layer effects onto the alpha-blending model: REPLACE turns
    // blending off; the blend-based effects (alphablend/normal/overlay/multiply)
    // turn it on. Overlay/multiply are approximated as normal alpha blending.
    if ($effect === IMG_EFFECT_REPLACE) {
        elephc_img_set_alpha_blending($image->handle, 0);
    } else {
        elephc_img_set_alpha_blending($image->handle, 1);
    }
    return true;
}

// ---- Drawing and fill -----------------------------------------------------

function imagesetthickness(GdImage $image, int $thickness): bool {
    elephc_img_set_thickness($image->handle, $thickness);
    return true;
}

function imageline(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool {
    elephc_img_line($image->handle, $x1, $y1, $x2, $y2, $color);
    return true;
}

function imagedashedline(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool {
    elephc_img_dashed_line($image->handle, $x1, $y1, $x2, $y2, $color);
    return true;
}

function imagerectangle(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool {
    elephc_img_rectangle($image->handle, $x1, $y1, $x2, $y2, $color);
    return true;
}

function imagefilledrectangle(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool {
    elephc_img_filled_rectangle($image->handle, $x1, $y1, $x2, $y2, $color);
    return true;
}

function imageellipse(GdImage $image, int $center_x, int $center_y, int $width, int $height, int $color): bool {
    elephc_img_ellipse($image->handle, $center_x, $center_y, $width, $height, $color);
    return true;
}

function imagefilledellipse(GdImage $image, int $center_x, int $center_y, int $width, int $height, int $color): bool {
    elephc_img_filled_ellipse($image->handle, $center_x, $center_y, $width, $height, $color);
    return true;
}

// The arc bridge calls pack (cx, cy) and (w, h) into single ints so they stay at
// six integer arguments — the x86_64 System V extern ABI only passes six integer
// arguments in registers, and elephc does not pass extern arguments on the stack.

function imagearc(GdImage $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color): bool {
    $_cxy = ($center_x << 32) | ($center_y & 0xFFFFFFFF);
    $_wh = ($width << 32) | ($height & 0xFFFFFFFF);
    elephc_img_arc($image->handle, $_cxy, $_wh, $start_angle, $end_angle, $color);
    return true;
}

function imagefilledarc(GdImage $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color, int $style): bool {
    $_cxy = ($center_x << 32) | ($center_y & 0xFFFFFFFF);
    $_wh = ($width << 32) | ($height & 0xFFFFFFFF);
    // The bridge entry fills a pie; IMG_ARC_NOFILL is routed to the arc outline
    // here (the edges of IMG_ARC_EDGED are approximated by that outline).
    if (($style & IMG_ARC_NOFILL) !== 0) {
        elephc_img_arc($image->handle, $_cxy, $_wh, $start_angle, $end_angle, $color);
    } else {
        elephc_img_filled_arc($image->handle, $_cxy, $_wh, $start_angle, $end_angle, $color);
    }
    return true;
}

function imagefill(GdImage $image, int $x, int $y, int $color): bool {
    elephc_img_fill($image->handle, $x, $y, $color);
    return true;
}

function imagefilltoborder(GdImage $image, int $x, int $y, int $border_color, int $color): bool {
    elephc_img_fill_to_border($image->handle, $x, $y, $border_color, $color);
    return true;
}

// The polygon point buffer is built inline in each function (a flat PHP points
// array [x0, y0, x1, y1, …] → repeated elephc_img_poly_add). The build loop is
// not factored into a shared helper because a single `array` parameter shared by
// callers passing different element types (Array(Int) literals vs Array(Mixed))
// hits the checker's array-parameter specialization, so each function owns its
// own loop.

function imagepolygon(GdImage $image, array $points, int $color): bool {
    elephc_img_poly_reset();
    $_n = count($points);
    $_i = 0;
    while ($_i + 1 < $_n) {
        elephc_img_poly_add((int) $points[$_i], (int) $points[$_i + 1]);
        $_i = $_i + 2;
    }
    elephc_img_poly_line($image->handle, $color, 1);
    return true;
}

function imageopenpolygon(GdImage $image, array $points, int $color): bool {
    elephc_img_poly_reset();
    $_n = count($points);
    $_i = 0;
    while ($_i + 1 < $_n) {
        elephc_img_poly_add((int) $points[$_i], (int) $points[$_i + 1]);
        $_i = $_i + 2;
    }
    elephc_img_poly_line($image->handle, $color, 0);
    return true;
}

function imagefilledpolygon(GdImage $image, array $points, int $color): bool {
    elephc_img_poly_reset();
    $_n = count($points);
    $_i = 0;
    while ($_i + 1 < $_n) {
        elephc_img_poly_add((int) $points[$_i], (int) $points[$_i + 1]);
        $_i = $_i + 2;
    }
    elephc_img_poly_fill($image->handle, $color);
    return true;
}

// ---- Built-in bitmap text -------------------------------------------------
// Every built-in font (1–5) renders with the same 8×8 glyph cell; the font
// number is accepted but does not change the size. The bridge takes the color
// before the text, so the PHP argument order (…, $string, $color) is swapped at
// the call.

function imagestring(GdImage $image, int $font, int $x, int $y, string $string, int $color): bool {
    elephc_img_string($image->handle, $font, $x, $y, $color, $string);
    return true;
}

function imagestringup(GdImage $image, int $font, int $x, int $y, string $string, int $color): bool {
    elephc_img_string_up($image->handle, $font, $x, $y, $color, $string);
    return true;
}

function imagechar(GdImage $image, int $font, int $x, int $y, string $char, int $color): bool {
    elephc_img_string($image->handle, $font, $x, $y, $color, substr($char, 0, 1));
    return true;
}

function imagecharup(GdImage $image, int $font, int $x, int $y, string $char, int $color): bool {
    elephc_img_string_up($image->handle, $font, $x, $y, $color, substr($char, 0, 1));
    return true;
}

function imagefontwidth(int $font): int {
    // All built-in fonts use a uniform 8×8 cell in elephc.
    $_unused = $font;
    return 8;
}

function imagefontheight(int $font): int {
    $_unused = $font;
    return 8;
}

// ---- Copy, scale, crop, flip, rotate, affine ------------------------------
// The copy/resize entry points pack each (x, y) / (width, height) pair into one
// integer so every extern stays within the six-integer-argument x86_64 ABI
// limit (the same packing as imagearc). Functions that produce a *new* image
// (scale/crop/cropauto/rotate/affine) return a plain GdImage and THROW an
// ImageException on failure, for the same reason imagecreatefrom* do — a
// GdImage|false result cannot be passed to a GdImage-typed function and is not
// narrowed after a === false check. See docs/php/image.md.

function imagecopy(GdImage $dst_image, GdImage $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height): bool {
    $_dxy = ($dst_x << 32) | ($dst_y & 0xFFFFFFFF);
    $_sxy = ($src_x << 32) | ($src_y & 0xFFFFFFFF);
    $_swh = ($src_width << 32) | ($src_height & 0xFFFFFFFF);
    return elephc_img_copy($dst_image->handle, $src_image->handle, $_dxy, $_sxy, $_swh) === 0;
}

function imagecopymerge(GdImage $dst_image, GdImage $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool {
    $_dxy = ($dst_x << 32) | ($dst_y & 0xFFFFFFFF);
    $_sxy = ($src_x << 32) | ($src_y & 0xFFFFFFFF);
    $_swh = ($src_width << 32) | ($src_height & 0xFFFFFFFF);
    return elephc_img_copy_merge($dst_image->handle, $src_image->handle, $_dxy, $_sxy, $_swh, $pct) === 0;
}

function imagecopymergegray(GdImage $dst_image, GdImage $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool {
    $_dxy = ($dst_x << 32) | ($dst_y & 0xFFFFFFFF);
    $_sxy = ($src_x << 32) | ($src_y & 0xFFFFFFFF);
    $_swh = ($src_width << 32) | ($src_height & 0xFFFFFFFF);
    return elephc_img_copy_merge_gray($dst_image->handle, $src_image->handle, $_dxy, $_sxy, $_swh, $pct) === 0;
}

function imagecopyresized(GdImage $dst_image, GdImage $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool {
    $_dxy = ($dst_x << 32) | ($dst_y & 0xFFFFFFFF);
    $_sxy = ($src_x << 32) | ($src_y & 0xFFFFFFFF);
    $_dwh = ($dst_width << 32) | ($dst_height & 0xFFFFFFFF);
    $_swh = ($src_width << 32) | ($src_height & 0xFFFFFFFF);
    return elephc_img_copy_resized($dst_image->handle, $src_image->handle, $_dxy, $_sxy, $_dwh, $_swh) === 0;
}

function imagecopyresampled(GdImage $dst_image, GdImage $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool {
    $_dxy = ($dst_x << 32) | ($dst_y & 0xFFFFFFFF);
    $_sxy = ($src_x << 32) | ($src_y & 0xFFFFFFFF);
    $_dwh = ($dst_width << 32) | ($dst_height & 0xFFFFFFFF);
    $_swh = ($src_width << 32) | ($src_height & 0xFFFFFFFF);
    return elephc_img_copy_resampled($dst_image->handle, $src_image->handle, $_dxy, $_sxy, $_dwh, $_swh) === 0;
}

function imagescale(GdImage $image, int $width, int $height = -1, int $mode = IMG_BILINEAR_FIXED): GdImage {
    $_handle = elephc_img_scale($image->handle, $width, $height, $mode);
    if ($_handle < 0) {
        throw new ImageException("imagescale(): invalid target dimensions");
    }
    return new GdImage($_handle);
}

function imagecrop(GdImage $image, $rect = ["x" => 0, "y" => 0, "width" => 0, "height" => 0]): GdImage {
    // $rect is the associative ["x" => , "y" => , "width" => , "height" => ]. The
    // parameter is left unhinted with an associative default (PHP types it `array`
    // and has no default; callers always pass one) so the checker infers a
    // string-keyed shape from the default — elephc models a plain `array` hint as
    // integer-keyed and rejects the string-key reads below when this function is
    // present but unused.
    $_x = (int) $rect["x"];
    $_y = (int) $rect["y"];
    $_w = (int) $rect["width"];
    $_h = (int) $rect["height"];
    $_handle = elephc_img_crop($image->handle, $_x, $_y, $_w, $_h);
    if ($_handle < 0) {
        throw new ImageException("imagecrop(): invalid crop rectangle");
    }
    return new GdImage($_handle);
}

function imagecropauto(GdImage $image, int $mode = IMG_CROP_DEFAULT, float $threshold = 0.5, int $color = -1): GdImage {
    // The threshold crosses as parts-per-thousand to keep the extern int-only.
    $_t = (int) round($threshold * 1000);
    $_handle = elephc_img_crop_auto($image->handle, $mode, $color, $_t);
    if ($_handle < 0) {
        throw new ImageException("imagecropauto(): nothing to crop or invalid image");
    }
    return new GdImage($_handle);
}

function imageflip(GdImage $image, int $mode): bool {
    return elephc_img_flip($image->handle, $mode) === 0;
}

function imagerotate(GdImage $image, float $angle, int $background_color, int $ignore_transparent = 0): GdImage {
    // $ignore_transparent is accepted for signature compatibility and ignored.
    $_unused = $ignore_transparent;
    // The angle crosses as millidegrees to keep the extern int-only.
    $_mdeg = (int) round($angle * 1000);
    $_handle = elephc_img_rotate($image->handle, $_mdeg, $background_color);
    if ($_handle < 0) {
        throw new ImageException("imagerotate(): invalid image");
    }
    return new GdImage($_handle);
}

function imageaffine(GdImage $image, array $affine, ?array $clip = null): GdImage {
    // $clip is accepted for signature compatibility but ignored: elephc always
    // returns the full transformed bounding box.
    $_unused = $clip;
    // Push the 6 matrix elements [a, b, c, d, e, f] as 16.16 fixed-point.
    elephc_img_fbuf_reset();
    $_i = 0;
    while ($_i < 6) {
        elephc_img_fbuf_push((int) round(((float) $affine[$_i]) * 65536));
        $_i = $_i + 1;
    }
    $_handle = elephc_img_affine($image->handle);
    if ($_handle < 0) {
        throw new ImageException("imageaffine(): invalid or singular affine matrix");
    }
    return new GdImage($_handle);
}

function imageaffinematrixconcat(array $matrix1, array $matrix2): array {
    // Compose two 2x3 affine matrices [a, b, c, d, e, f], matching gdAffineConcat.
    $_a1 = (float) $matrix1[0];
    $_b1 = (float) $matrix1[1];
    $_c1 = (float) $matrix1[2];
    $_d1 = (float) $matrix1[3];
    $_e1 = (float) $matrix1[4];
    $_f1 = (float) $matrix1[5];
    $_a2 = (float) $matrix2[0];
    $_b2 = (float) $matrix2[1];
    $_c2 = (float) $matrix2[2];
    $_d2 = (float) $matrix2[3];
    $_e2 = (float) $matrix2[4];
    $_f2 = (float) $matrix2[5];
    return [
        $_a1 * $_a2 + $_b1 * $_c2,
        $_a1 * $_b2 + $_b1 * $_d2,
        $_c1 * $_a2 + $_d1 * $_c2,
        $_c1 * $_b2 + $_d1 * $_d2,
        $_e1 * $_a2 + $_f1 * $_c2 + $_e2,
        $_e1 * $_b2 + $_f1 * $_d2 + $_f2,
    ];
}

// ---- Filters, convolution, gamma, interpolation ---------------------------

function imagefilter(GdImage $image, int $filter, int $arg1 = 0, int $arg2 = 0, int $arg3 = 0, int $arg4 = 0): bool {
    // PHP's variadic ...$args map to the four fixed slots; the numeric filters
    // (brightness/contrast/colorize/smooth/pixelate/scatter) use them, and the
    // colors-array form of IMG_FILTER_SCATTER is not supported (documented).
    return elephc_img_filter($image->handle, $filter, $arg1, $arg2, $arg3, $arg4) === 0;
}

function imageconvolution(GdImage $image, array $matrix, float $divisor, float $offset): bool {
    // Push the 3x3 kernel row-major as 16.16 fixed-point.
    elephc_img_fbuf_reset();
    $_r = 0;
    while ($_r < 3) {
        $_c = 0;
        while ($_c < 3) {
            elephc_img_fbuf_push((int) round(((float) $matrix[$_r][$_c]) * 65536));
            $_c = $_c + 1;
        }
        $_r = $_r + 1;
    }
    $_div = (int) round($divisor * 65536);
    $_off = (int) round($offset * 65536);
    return elephc_img_convolution($image->handle, $_div, $_off) === 0;
}

function imagegammacorrect(GdImage $image, float $input_gamma, float $output_gamma): bool {
    $_in = (int) round($input_gamma * 65536);
    $_out = (int) round($output_gamma * 65536);
    return elephc_img_gamma($image->handle, $_in, $_out) === 0;
}

function imagesetinterpolation(GdImage $image, int $method = IMG_BILINEAR_FIXED): bool {
    elephc_img_set_interpolation($image->handle, $method);
    return true;
}

function imagegetinterpolation(GdImage $image): int {
    return elephc_img_get_interpolation($image->handle);
}

function imageinterlace(GdImage $image, ?bool $enable = null): int {
    if ($enable !== null) {
        elephc_img_set_interlace($image->handle, $enable ? 1 : 0);
    }
    return elephc_img_get_interlace($image->handle);
}

function imageantialias(GdImage $image, bool $enable): bool {
    // Antialiased primitive drawing is not implemented (documented gap); the call
    // is accepted as a no-op and returns true like GD on a build that supports it.
    $_unused = $image;
    $_u2 = $enable;
    return true;
}

// Internal format codes shared with the bridge (see FMT_* in the bridge crate):
// 1=PNG 2=JPEG 3=GIF 4=BMP 5=WEBP. imagecreatefrom* pass the expected format so a
// mismatched file (e.g. a JPEG fed to imagecreatefrompng) is rejected like GD.
//
// PHP-compatibility note: PHP's imagecreatefrom*/imagecreatefromstring return
// `GdImage|false` on failure. elephc cannot return a `GdImage|false` that stays
// usable, because the result must be passed to a `GdImage`-typed function and the
// checker neither accepts a `GdImage|bool` argument there nor narrows it after a
// `=== false` guard (the union-value-runtime limitation). So these functions
// return a plain `GdImage` and THROW an `ImageException` on failure instead of
// returning false; the common `$im = imagecreatefrompng($f); imagesx($im);` flow
// then type-checks, and error handling uses try/catch. See docs/php/image.md.

class ImageException extends RuntimeException {
}

function imagecreatefrompng(string $filename): GdImage {
    $_h = elephc_img_create_from_file($filename, 1);
    if ($_h < 0) {
        throw new ImageException("imagecreatefrompng(): failed to open or decode '" . $filename . "'");
    }
    return new GdImage($_h);
}

function imagecreatefromjpeg(string $filename): GdImage {
    $_h = elephc_img_create_from_file($filename, 2);
    if ($_h < 0) {
        throw new ImageException("imagecreatefromjpeg(): failed to open or decode '" . $filename . "'");
    }
    return new GdImage($_h);
}

function imagecreatefromgif(string $filename): GdImage {
    $_h = elephc_img_create_from_file($filename, 3);
    if ($_h < 0) {
        throw new ImageException("imagecreatefromgif(): failed to open or decode '" . $filename . "'");
    }
    return new GdImage($_h);
}

function imagecreatefrombmp(string $filename): GdImage {
    $_h = elephc_img_create_from_file($filename, 4);
    if ($_h < 0) {
        throw new ImageException("imagecreatefrombmp(): failed to open or decode '" . $filename . "'");
    }
    return new GdImage($_h);
}

function imagecreatefromwebp(string $filename): GdImage {
    $_h = elephc_img_create_from_file($filename, 5);
    if ($_h < 0) {
        throw new ImageException("imagecreatefromwebp(): failed to open or decode '" . $filename . "'");
    }
    return new GdImage($_h);
}

function imagecreatefromtga(string $filename): GdImage {
    $_h = elephc_img_create_from_file($filename, 6);
    if ($_h < 0) {
        throw new ImageException("imagecreatefromtga(): failed to open or decode '" . $filename . "'");
    }
    return new GdImage($_h);
}

function imagecreatefromstring(string $data): GdImage {
    $_len = strlen($data);
    if ($_len <= 0) {
        throw new ImageException("imagecreatefromstring(): empty image data");
    }
    // Copy the PHP string into the bridge's staging buffer, then decode it there
    // (auto-detecting the format, as PHP does). ptr_write_string is binary-safe,
    // so embedded NUL bytes in the encoded image survive the transfer.
    $_buf = elephc_img_stage_ptr($_len);
    if (ptr_is_null($_buf)) {
        throw new ImageException("imagecreatefromstring(): could not allocate decode buffer");
    }
    ptr_write_string($_buf, $data);
    $_h = elephc_img_create_from_stage($_len);
    if ($_h < 0) {
        throw new ImageException("imagecreatefromstring(): data is not a recognized image");
    }
    return new GdImage($_h);
}

// Shared output path for the image{png,jpeg,gif,bmp,webp} family. With a file
// path the bridge encodes straight to disk; with null the bridge encodes into its
// cell and we copy the bytes out (binary-safe via ptr_read_string) and echo them,
// matching GD's "write the image to stdout" behavior for a null filename.
function _elephc_img_output(int $handle, int $fmt, ?string $file, int $quality): bool {
    if ($file !== null) {
        return elephc_img_write_file($handle, $fmt, (string) $file, $quality) === 0;
    }
    if (elephc_img_encode($handle, $fmt, $quality) !== 0) {
        return false;
    }
    $_len = elephc_img_encoded_len();
    $_ptr = elephc_img_encoded_ptr();
    $_bytes = ptr_read_string($_ptr, $_len);
    elephc_img_encoded_clear();
    echo $_bytes;
    return true;
}

function imagepng(GdImage $image, ?string $file = null, int $quality = -1, int $filters = -1): bool {
    // PNG is lossless; GD's $quality (zlib level) and $filters do not change
    // pixels, so they are accepted for signature compatibility and ignored.
    $_unused = $filters;
    return _elephc_img_output($image->handle, 1, $file, $quality);
}

function imagejpeg(GdImage $image, ?string $file = null, int $quality = -1): bool {
    return _elephc_img_output($image->handle, 2, $file, $quality);
}

function imagegif(GdImage $image, ?string $file = null): bool {
    return _elephc_img_output($image->handle, 3, $file, -1);
}

function imagebmp(GdImage $image, ?string $file = null, bool $compressed = true): bool {
    // The bundled BMP encoder always writes uncompressed BMP; $compressed is
    // accepted for signature compatibility and ignored.
    $_unused = $compressed;
    return _elephc_img_output($image->handle, 4, $file, -1);
}

function imagewebp(GdImage $image, ?string $file = null, int $quality = -1): bool {
    // The bundled WebP encoder is lossless, so $quality is accepted for signature
    // compatibility and ignored.
    return _elephc_img_output($image->handle, 5, $file, $quality);
}

function imagetypes(): int {
    return IMG_GIF | IMG_JPG | IMG_PNG | IMG_WEBP | IMG_BMP;
}

function gd_info(): array {
    // Capabilities of the bundled pure-Rust backend. FreeType/TTF text is a
    // documented gap (no bundled cross-platform test font); WBMP/XPM/XBM, the
    // native GD/GD2 formats, and AVIF decode have no pure-Rust path (documented
    // gaps), so they are reported unsupported. TGA read is supported.
    return [
        "GD Version" => "bundled (pure-Rust, 2.1.0 compatible)",
        "FreeType Support" => false,
        "FreeType Linkage" => "",
        "GIF Read Support" => true,
        "GIF Create Support" => true,
        "JPEG Support" => true,
        "PNG Support" => true,
        "WBMP Support" => false,
        "XPM Support" => false,
        "XBM Support" => false,
        "WebP Support" => true,
        "BMP Support" => true,
        "AVIF Support" => false,
        "TGA Read Support" => true,
        "JIS-mapped Japanese Font Support" => false,
    ];
}

function image_type_to_mime_type(int $image_type): string {
    switch ($image_type) {
        case IMAGETYPE_GIF: return "image/gif";
        case IMAGETYPE_JPEG: return "image/jpeg";
        case IMAGETYPE_PNG: return "image/png";
        case IMAGETYPE_SWF: return "application/x-shockwave-flash";
        case IMAGETYPE_PSD: return "image/psd";
        case IMAGETYPE_BMP: return "image/bmp";
        case IMAGETYPE_TIFF_II: return "image/tiff";
        case IMAGETYPE_TIFF_MM: return "image/tiff";
        case IMAGETYPE_JPC: return "application/octet-stream";
        case IMAGETYPE_JP2: return "image/jp2";
        case IMAGETYPE_JPX: return "application/octet-stream";
        case IMAGETYPE_JB2: return "application/octet-stream";
        case IMAGETYPE_SWC: return "application/x-shockwave-flash";
        case IMAGETYPE_IFF: return "image/iff";
        case IMAGETYPE_WBMP: return "image/vnd.wap.wbmp";
        case IMAGETYPE_XBM: return "image/xbm";
        case IMAGETYPE_ICO: return "image/vnd.microsoft.icon";
        case IMAGETYPE_WEBP: return "image/webp";
        case IMAGETYPE_AVIF: return "image/avif";
        default: return "application/octet-stream";
    }
}

// Known limitation: PHP returns `string|false`, returning `false` for an unknown
// type. elephc currently collapses a string|false function return to `string`
// (coercing the `false` to ""), and an explicit union return type hits an
// unsupported EIR path for the dead-code copy of this function. So an unknown
// type yields "" here rather than `false`. Revisit when scalar-union values are
// representable end-to-end (the union-value-runtime work).
function image_type_to_extension(int $image_type, bool $include_dot = true) {
    $ext = "";
    switch ($image_type) {
        case IMAGETYPE_GIF: $ext = "gif"; break;
        case IMAGETYPE_JPEG: $ext = "jpeg"; break;
        case IMAGETYPE_PNG: $ext = "png"; break;
        case IMAGETYPE_SWF: $ext = "swf"; break;
        case IMAGETYPE_PSD: $ext = "psd"; break;
        case IMAGETYPE_BMP: $ext = "bmp"; break;
        case IMAGETYPE_TIFF_II: $ext = "tiff"; break;
        case IMAGETYPE_TIFF_MM: $ext = "tiff"; break;
        case IMAGETYPE_JPC: $ext = "jpc"; break;
        case IMAGETYPE_JP2: $ext = "jp2"; break;
        case IMAGETYPE_JPX: $ext = "jpx"; break;
        case IMAGETYPE_JB2: $ext = "jb2"; break;
        case IMAGETYPE_IFF: $ext = "iff"; break;
        case IMAGETYPE_WBMP: $ext = "wbmp"; break;
        case IMAGETYPE_XBM: $ext = "xbm"; break;
        case IMAGETYPE_ICO: $ext = "ico"; break;
        case IMAGETYPE_WEBP: $ext = "webp"; break;
        case IMAGETYPE_AVIF: $ext = "avif"; break;
        default: return false;
    }
    if ($include_dot) {
        return "." . $ext;
    }
    return $ext;
}

function getimagesize(string $filename) {
    if (elephc_img_probe_file($filename) !== 0) {
        return false;
    }
    $w = elephc_img_probe_width();
    $h = elephc_img_probe_height();
    $type = elephc_img_probe_type();
    $bits = elephc_img_probe_bits();
    $channels = elephc_img_probe_channels();
    return [
        0 => $w,
        1 => $h,
        2 => $type,
        3 => "width=\"" . $w . "\" height=\"" . $h . "\"",
        "bits" => $bits,
        "channels" => $channels,
        "mime" => image_type_to_mime_type($type),
    ];
}

// getimagesizefromstring: the same array shape as getimagesize, but the image
// bytes arrive in a string (staged into the bridge buffer) instead of a file
// path. The optional &$image_info APP-markers parameter PHP accepts is omitted
// (elephc does not surface APP/EXIF meta here); call exif_read_data() for tags.
function getimagesizefromstring(string $data) {
    $_len = strlen($data);
    if ($_len <= 0) {
        return false;
    }
    $_buf = elephc_img_stage_ptr($_len);
    if (ptr_is_null($_buf)) {
        return false;
    }
    ptr_write_string($_buf, $data);
    if (elephc_img_probe_stage($_len) !== 0) {
        return false;
    }
    $w = elephc_img_probe_width();
    $h = elephc_img_probe_height();
    $type = elephc_img_probe_type();
    $bits = elephc_img_probe_bits();
    $channels = elephc_img_probe_channels();
    return [
        0 => $w,
        1 => $h,
        2 => $type,
        3 => "width=\"" . $w . "\" height=\"" . $h . "\"",
        "bits" => $bits,
        "channels" => $channels,
        "mime" => image_type_to_mime_type($type),
    ];
}

// === Exif + IPTC ===========================================================
//
// exif_read_data parses a file's EXIF attributes into a flat associative array
// keyed by the standard EXIF mnemonics (Make, Model, Orientation, DateTime, ...).
// PHP-compatibility notes (documented simplifications, see docs/php/image.md):
//   * Values are returned as strings (ASCII text, integers as decimals, rationals
//     as "num/den"); PHP returns typed scalars/arrays. A key appearing in more
//     than one IFD keeps its last value.
//   * The synthetic FILE / COMPUTED / SectionsFound meta-sections PHP injects are
//     not produced; use getimagesize()/exif_imagetype() for file-level data.
//   * $required_sections, $as_arrays, and $read_thumbnail are accepted for
//     signature compatibility and do not change the returned tags.

const EXIF_USE_MBSTRING = 0;

function exif_imagetype(string $filename) {
    // Header sniff via the bridge's probe; returns the IMAGETYPE_* code or false
    // for an unreadable/unrecognized file, matching PHP's exif_imagetype().
    if (elephc_img_probe_file($filename) !== 0) {
        return false;
    }
    return elephc_img_probe_type();
}

// PHP returns string|false (false for an unknown tag). elephc collapses a
// string|false return to string, so an unknown tag yields "" here rather than
// false — the same limitation documented on image_type_to_extension. Test with
// `=== ""` instead of `=== false`.
function exif_tagname(int $index) {
    $_len = elephc_exif_tagname($index);
    if ($_len < 0) {
        return false;
    }
    return ptr_read_string(elephc_img_out_ptr(), $_len);
}

function exif_read_data(string $filename, ?string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false) {
    $_unused_a = $required_sections;
    $_unused_b = $as_arrays;
    $_unused_c = $read_thumbnail;
    $_count = elephc_exif_read($filename);
    if ($_count < 0) {
        return false;
    }
    // Build with string-keyed writes only (the type checker pins an empty [] to
    // integer keys for any string-keyed read, so we never read $result by key).
    $result = [];
    $_n = elephc_img_kv_count();
    for ($_i = 0; $_i < $_n; $_i++) {
        $_klen = elephc_img_kv_key($_i);
        $_key = ptr_read_string(elephc_img_out_ptr(), $_klen);
        $_vlen = elephc_img_kv_val($_i);
        $_val = ptr_read_string(elephc_img_out_ptr(), $_vlen);
        $result[$_key] = $_val;
    }
    return $result;
}

function read_exif_data(string $filename, ?string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false) {
    return exif_read_data($filename, $required_sections, $as_arrays, $read_thumbnail);
}

// PHP returns string|false (false when there is no thumbnail). As with
// exif_tagname, elephc collapses string|false to string, so the no-thumbnail case
// yields "" (and leaves the by-ref width/height/image_type untouched) rather than
// false. Test the result with `=== ""` / strlen() instead of `=== false`.
function exif_thumbnail(string $filename, &$width = 0, &$height = 0, &$image_type = 0) {
    $_len = elephc_exif_thumbnail($filename);
    if ($_len < 0) {
        return false;
    }
    $_bytes = ptr_read_string(elephc_img_out_ptr(), $_len);
    $width = elephc_exif_thumb_width();
    $height = elephc_exif_thumb_height();
    $image_type = elephc_exif_thumb_type();
    return $_bytes;
}

function iptcparse(string $iptcblock) {
    $_len = strlen($iptcblock);
    if ($_len <= 0) {
        return false;
    }
    $_buf = elephc_img_in_ptr($_len);
    if (ptr_is_null($_buf)) {
        return false;
    }
    ptr_write_string($_buf, $iptcblock);
    $_keys = elephc_iptc_parse($_len);
    if ($_keys < 0) {
        return false;
    }
    // Each key holds an array of values; build each sub-array fully, then assign
    // it under the string key (string-keyed write only).
    $result = [];
    for ($_i = 0; $_i < $_keys; $_i++) {
        $_klen = elephc_iptc_key($_i);
        $_key = ptr_read_string(elephc_img_out_ptr(), $_klen);
        $_nv = elephc_iptc_val_count($_i);
        $_sub = [];
        for ($_j = 0; $_j < $_nv; $_j++) {
            $_vlen = elephc_iptc_val($_i, $_j);
            $_sub[] = ptr_read_string(elephc_img_out_ptr(), $_vlen);
        }
        $result[$_key] = $_sub;
    }
    return $result;
}

function iptcembed(string $iptcdata, string $jpeg_file_name, int $spool = 0) {
    // Embed the IPTC block into the JPEG as a Photoshop APP13 marker. PHP's $spool
    // selects writing to stdout vs returning the bytes; elephc always returns the
    // new JPEG as a string, and additionally echoes it when $spool >= 2.
    $_len = strlen($iptcdata);
    $_buf = elephc_img_in_ptr($_len);
    if (ptr_is_null($_buf)) {
        return false;
    }
    ptr_write_string($_buf, $iptcdata);
    $_outlen = elephc_iptc_embed($jpeg_file_name, $_len);
    if ($_outlen < 0) {
        return false;
    }
    $_bytes = ptr_read_string(elephc_img_out_ptr(), $_outlen);
    if ($spool >= 2) {
        echo $_bytes;
    }
    return $_bytes;
}

// ---------------------------------------------------------------------------
// Imagick OOP surface: Imagick, ImagickDraw, ImagickPixel,
// ImagickPixelIterator, ImagickKernel. These are a pure-Rust semantic
// reimplementation of the PHP Imagick extension over the same bridge that backs
// GD: an Imagick wand is a sequence of frames (GD images) and per-image methods
// reuse the GD operations. They reproduce documented behavior but are NOT
// byte-identical to ImageMagick; operators with no pure-Rust equivalent throw
// ImagickException. See docs/php/image.md.
// ---------------------------------------------------------------------------

class ImagickException extends Exception {
}

class ImagickDrawException extends Exception {
}

class ImagickPixelException extends Exception {
}

class ImagickPixelIteratorException extends Exception {
}

class ImagickKernelException extends Exception {
}

// Parses an unsigned hexadecimal string into an integer. elephc has no hexdec(),
// and string comparison requires numeric operands, so digits are classified by
// their ASCII code via ord().
function _imagick_hexval(string $hex): int {
    $_n = strlen($hex);
    $_acc = 0;
    for ($_i = 0; $_i < $_n; $_i++) {
        $_o = ord($hex[$_i]);
        $_d = 0;
        if ($_o >= 48 && $_o <= 57) {
            $_d = $_o - 48;
        } elseif ($_o >= 97 && $_o <= 102) {
            $_d = $_o - 87;
        } elseif ($_o >= 65 && $_o <= 70) {
            $_d = $_o - 55;
        }
        $_acc = $_acc * 16 + $_d;
    }
    return $_acc;
}

// Maps a CSS/X11 color name (lowercase) to a GD packed RGB color, or -1 if the
// name is unknown. "transparent"/"none" use GD's fully-transparent encoding.
function _imagick_color_name(string $name): int {
    switch ($name) {
        case "black": return 0;
        case "white": return 16777215;
        case "red": return 16711680;
        case "lime": return 65280;
        case "green": return 32768;
        case "blue": return 255;
        case "yellow": return 16776960;
        case "cyan":
        case "aqua": return 65535;
        case "magenta":
        case "fuchsia": return 16711935;
        case "silver": return 12632256;
        case "gray":
        case "grey": return 8421504;
        case "maroon": return 8388608;
        case "olive": return 8421376;
        case "purple": return 8388736;
        case "teal": return 32896;
        case "navy": return 128;
        case "orange": return 16753920;
        case "pink": return 16761035;
        case "brown": return 10824234;
        case "gold": return 16766720;
        case "violet": return 15631086;
        case "indigo": return 4915330;
        case "transparent":
        case "none": return 2130706432;
        default: return -1;
    }
}

// Parses an Imagick/CSS color string into a GD packed color: #rgb / #rrggbb /
// #rrggbbaa, rgb()/rgba(), or a named color. Throws ImagickPixelException for an
// unrecognized color.
function _imagick_parse_color(string $c): int {
    $_c = trim($c);
    if ($_c === "") {
        return 0;
    }
    if (ord($_c[0]) === 35) {
        $_hex = substr($_c, 1);
        $_len = strlen($_hex);
        if ($_len === 3) {
            $_r = _imagick_hexval($_hex[0] . $_hex[0]);
            $_g = _imagick_hexval($_hex[1] . $_hex[1]);
            $_b = _imagick_hexval($_hex[2] . $_hex[2]);
            return ($_r << 16) | ($_g << 8) | $_b;
        }
        if ($_len === 6) {
            $_r = _imagick_hexval(substr($_hex, 0, 2));
            $_g = _imagick_hexval(substr($_hex, 2, 2));
            $_b = _imagick_hexval(substr($_hex, 4, 2));
            return ($_r << 16) | ($_g << 8) | $_b;
        }
        if ($_len === 8) {
            $_r = _imagick_hexval(substr($_hex, 0, 2));
            $_g = _imagick_hexval(substr($_hex, 2, 2));
            $_b = _imagick_hexval(substr($_hex, 4, 2));
            $_a8 = _imagick_hexval(substr($_hex, 6, 2));
            $_gd = (int) ((255 - $_a8) * 127 / 255);
            return ($_gd << 24) | ($_r << 16) | ($_g << 8) | $_b;
        }
        throw new ImagickPixelException("ImagickPixel: malformed hex color '" . $c . "'");
    }
    $_lower = strtolower($_c);
    if (substr($_lower, 0, 4) === "rgb(" || substr($_lower, 0, 5) === "rgba(") {
        $_open = strpos($_c, "(");
        $_close = strpos($_c, ")");
        $_inner = substr($_c, $_open + 1, $_close - $_open - 1);
        $_parts = explode(",", $_inner);
        $_r = (int) trim($_parts[0]);
        $_g = (int) trim($_parts[1]);
        $_b = (int) trim($_parts[2]);
        $_gd = 0;
        if (count($_parts) >= 4) {
            $_af = (float) trim($_parts[3]);
            $_gd = (int) ((1.0 - $_af) * 127);
        }
        return ($_gd << 24) | ($_r << 16) | ($_g << 8) | $_b;
    }
    $_named = _imagick_color_name($_lower);
    if ($_named >= 0) {
        return $_named;
    }
    throw new ImagickPixelException("ImagickPixel: unrecognized color '" . $c . "'");
}

// Normalizes a color argument that may be a string or an ImagickPixel into a GD
// packed color. The (int) cast resolves the ImagickPixel branch's Mixed property
// read to an int.
function _imagick_norm_color($color): int {
    if (is_string($color)) {
        return _imagick_parse_color($color);
    }
    // instanceof narrows the Mixed argument to an object so the property read is
    // allowed; the (int) cast resolves the Mixed property type back to int.
    if ($color instanceof ImagickPixel) {
        return (int) $color->packed;
    }
    return 0;
}

// Maps an Imagick format string (case-insensitive) to the bridge FMT_* code, or
// 0 when the format is not one the pure-Rust encoder supports.
function _imagick_fmt_to_code(string $format): int {
    $_f = strtoupper($format);
    if ($_f === "PNG") { return 1; }
    if ($_f === "JPEG" || $_f === "JPG") { return 2; }
    if ($_f === "GIF") { return 3; }
    if ($_f === "BMP") { return 4; }
    if ($_f === "WEBP") { return 5; }
    return 0;
}

// Maps a bridge FMT_* code back to its canonical Imagick format string.
function _imagick_code_to_fmt(int $code): string {
    if ($code === 1) { return "PNG"; }
    if ($code === 2) { return "JPEG"; }
    if ($code === 3) { return "GIF"; }
    if ($code === 4) { return "BMP"; }
    if ($code === 5) { return "WEBP"; }
    return "";
}

// Derives a FMT_* code from a file path's extension, or 0 when absent/unknown.
function _imagick_fmt_from_path(string $path): int {
    $_dot = strrpos($path, ".");
    if ($_dot === false) {
        return 0;
    }
    return _imagick_fmt_to_code(substr($path, $_dot + 1));
}

// Packs two 32-bit values into one int (hi << 32 | lo) for the bridge entry
// points that take packed coordinate/size/angle pairs.
function _imagick_pack2(int $hi, int $lo): int {
    return (($hi & 0xFFFFFFFF) << 32) | ($lo & 0xFFFFFFFF);
}

// Wraps a GD packed color into a fresh ImagickPixel (used by getImagePixelColor
// and the pixel iterator).
function _imagick_pixel_from_int(int $packed): ImagickPixel {
    $_p = new ImagickPixel("black");
    $_p->packed = $packed;
    return $_p;
}

class ImagickPixel {
    public int $packed = 0;

    public function __construct(string $color = "black") {
        $this->packed = _imagick_parse_color($color);
    }

    public function setColor(string $color): bool {
        $this->packed = _imagick_parse_color($color);
        return true;
    }

    // Returns the color as an associative array. With $normalized != 0 the
    // channels are 0..1 floats; otherwise 0..255 integers. No return type hint so
    // the inferred associative type lets callers read the "r"/"g"/"b"/"a" keys.
    public function getColor(int $normalized = 0) {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        $_gd = ($this->packed >> 24) & 0x7F;
        $_a = 255 - (int) ($_gd * 255 / 127);
        if ($normalized !== 0) {
            return ["r" => $_r / 255, "g" => $_g / 255, "b" => $_b / 255, "a" => $_a / 255];
        }
        return ["r" => $_r, "g" => $_g, "b" => $_b, "a" => $_a];
    }

    // Returns one channel as a 0..1 float, selected by an Imagick::COLOR_* code.
    public function getColorValue(int $color): float {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        $_gd = ($this->packed >> 24) & 0x7F;
        $_a = 255 - (int) ($_gd * 255 / 127);
        if ($color === 4) { return $_r / 255; }
        if ($color === 3) { return $_g / 255; }
        if ($color === 1) { return $_b / 255; }
        if ($color === 8) { return $_a / 255; }
        if ($color === 7) { return (255 - $_a) / 255; }
        return 0.0;
    }

    // Returns the color as an "srgb(r,g,b)" string, matching Imagick's textual
    // form for an opaque color.
    public function getColorAsString(): string {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        return "srgb(" . $_r . "," . $_g . "," . $_b . ")";
    }

    // Returns whether two colors are within $fuzz (0..1) RGB distance.
    public function isSimilar(ImagickPixel $color, float $fuzz): bool {
        $_dr = (($this->packed >> 16) & 0xFF) - (($color->packed >> 16) & 0xFF);
        $_dg = (($this->packed >> 8) & 0xFF) - (($color->packed >> 8) & 0xFF);
        $_db = ($this->packed & 0xFF) - ($color->packed & 0xFF);
        $_dist = sqrt((float) ($_dr * $_dr + $_dg * $_dg + $_db * $_db)) / (sqrt(3.0) * 255);
        return $_dist <= $fuzz;
    }

    public function isPixelSimilar(ImagickPixel $color, float $fuzz): bool {
        return $this->isSimilar($color, $fuzz);
    }

    public function clear(): bool {
        return true;
    }

    public function destroy(): bool {
        return true;
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function getColorCount(): int {
        throw new ImagickPixelException("ImagickPixel::getColorCount() is not supported in elephc");
    }
    public function getColorQuantum(): array {
        throw new ImagickPixelException("ImagickPixel::getColorQuantum() is not supported in elephc");
    }
    public function getColorValueQuantum(int $color): int|float {
        $_u_color = $color;
        throw new ImagickPixelException("ImagickPixel::getColorValueQuantum() is not supported in elephc");
    }
    public function getHSL(): array {
        throw new ImagickPixelException("ImagickPixel::getHSL() is not supported in elephc");
    }
    public function getIndex(): int {
        throw new ImagickPixelException("ImagickPixel::getIndex() is not supported in elephc");
    }
    public function isPixelSimilarQuantum(string $color, string $fuzz = ""): bool {
        $_u_color = $color;
        $_u_fuzz = $fuzz;
        throw new ImagickPixelException("ImagickPixel::isPixelSimilarQuantum() is not supported in elephc");
    }
    public function setcolorcount(int $colorCount): bool {
        $_u_colorCount = $colorCount;
        throw new ImagickPixelException("ImagickPixel::setcolorcount() is not supported in elephc");
    }
    public function setColorValue(int $color, float $value): bool {
        $_u_color = $color;
        $_u_value = $value;
        throw new ImagickPixelException("ImagickPixel::setColorValue() is not supported in elephc");
    }
    public function setColorValueQuantum(int $color, int|float $value): bool {
        $_u_color = $color;
        $_u_value = $value;
        throw new ImagickPixelException("ImagickPixel::setColorValueQuantum() is not supported in elephc");
    }
    public function setHSL(float $hue, float $saturation, float $luminosity): bool {
        $_u_hue = $hue;
        $_u_saturation = $saturation;
        $_u_luminosity = $luminosity;
        throw new ImagickPixelException("ImagickPixel::setHSL() is not supported in elephc");
    }
    public function setIndex(int $index): bool {
        $_u_index = $index;
        throw new ImagickPixelException("ImagickPixel::setIndex() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---

}

class ImagickKernel {
    public int $size = 0;
    public array $values = [];
    public float $divisor = 1.0;

    // Builds a kernel from a square matrix of weights (row-major). The divisor is
    // the weight sum (1.0 when the sum is zero, e.g. an edge kernel).
    public static function fromMatrix(array $matrix): ImagickKernel {
        $_k = new ImagickKernel();
        $_n = count($matrix);
        $_k->size = $_n;
        $_sum = 0.0;
        for ($_r = 0; $_r < $_n; $_r++) {
            for ($_c = 0; $_c < $_n; $_c++) {
                $_v = (float) $matrix[$_r][$_c];
                $_k->values[] = $_v;
                $_sum = $_sum + $_v;
            }
        }
        $_k->divisor = $_sum == 0.0 ? 1.0 : $_sum;
        return $_k;
    }

    // Built-in kernels are not available on the pure-Rust backend; use fromMatrix.
    public static function fromBuiltIn(int $kernelType, string $kernelString): ImagickKernel {
        $_u_t = $kernelType;
        $_u_s = $kernelString;
        throw new ImagickKernelException("ImagickKernel::fromBuiltIn() is not supported in elephc; use fromMatrix()");
    }

    public function getMatrix(): array {
        return $this->values;
    }

    // Internal accessors used by Imagick::convolveImage.
    public function _size(): int {
        return $this->size;
    }

    public function _at(int $i): float {
        return (float) $this->values[$i];
    }

    public function _divisor(): float {
        return $this->divisor;
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function addKernel(ImagickKernel $imagickKernel): void {
        $_u_imagickKernel = $imagickKernel;
        throw new ImagickKernelException("ImagickKernel::addKernel() is not supported in elephc");
    }
    public function addUnityKernel(float $scale): void {
        $_u_scale = $scale;
        throw new ImagickKernelException("ImagickKernel::addUnityKernel() is not supported in elephc");
    }
    public function scale(float $scale, int $normalizeFlag = 0): void {
        $_u_scale = $scale;
        $_u_normalizeFlag = $normalizeFlag;
        throw new ImagickKernelException("ImagickKernel::scale() is not supported in elephc");
    }
    public function separate(): array {
        throw new ImagickKernelException("ImagickKernel::separate() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---

}

class ImagickDraw {
    private int $draw = 0;

    public function __construct() {
        $this->draw = elephc_idraw_new();
    }

    // Internal: exposes the bridge draw handle to Imagick::drawImage.
    public function _imagickHandle(): int {
        return $this->draw;
    }

    public function setFillColor($fill): bool {
        elephc_idraw_set_fill($this->draw, _imagick_norm_color($fill));
        return true;
    }

    public function setStrokeColor($stroke): bool {
        elephc_idraw_set_stroke($this->draw, _imagick_norm_color($stroke));
        return true;
    }

    public function setStrokeWidth(float $width): bool {
        elephc_idraw_set_stroke_width($this->draw, (int) round($width));
        return true;
    }

    public function getFillColor(): ImagickPixel {
        return _imagick_pixel_from_int(elephc_idraw_get_fill($this->draw));
    }

    public function line($sx, $sy, $ex, $ey): bool {
        elephc_idraw_line($this->draw, (int) round($sx), (int) round($sy), (int) round($ex), (int) round($ey));
        return true;
    }

    public function rectangle($x1, $y1, $x2, $y2): bool {
        elephc_idraw_rectangle($this->draw, (int) round($x1), (int) round($y1), (int) round($x2), (int) round($y2));
        return true;
    }

    public function circle($ox, $oy, $px, $py): bool {
        elephc_idraw_circle($this->draw, (int) round($ox), (int) round($oy), (int) round($px), (int) round($py));
        return true;
    }

    public function ellipse($ox, $oy, $rx, $ry, $start, $end): bool {
        $_oxy = _imagick_pack2((int) round($ox), (int) round($oy));
        $_rxy = _imagick_pack2((int) round($rx), (int) round($ry));
        $_se = _imagick_pack2((int) round($start), (int) round($end));
        elephc_idraw_ellipse($this->draw, $_oxy, $_rxy, $_se);
        return true;
    }

    public function point($x, $y): bool {
        elephc_idraw_point($this->draw, (int) round($x), (int) round($y));
        return true;
    }

    // Draws a filled/stroked polygon. Each coordinate is ["x" => , "y" => ].
    public function polygon(array $coordinates): bool {
        elephc_idraw_poly_reset($this->draw);
        $_n = count($coordinates);
        for ($_i = 0; $_i < $_n; $_i++) {
            $_px = (int) round($coordinates[$_i]["x"]);
            $_py = (int) round($coordinates[$_i]["y"]);
            elephc_idraw_poly_point($this->draw, $_px, $_py);
        }
        elephc_idraw_polygon($this->draw);
        return true;
    }

    public function clear(): bool {
        elephc_idraw_clear($this->draw);
        return true;
    }

    public function destroy(): bool {
        elephc_idraw_destroy($this->draw);
        $this->draw = 0;
        return true;
    }

    public function __destruct() {
        elephc_idraw_destroy($this->draw);
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function affine(array $affine): bool {
        $_u_affine = $affine;
        throw new ImagickDrawException("ImagickDraw::affine() is not supported in elephc");
    }
    public function annotation(float $x, float $y, string $text): bool {
        $_u_x = $x;
        $_u_y = $y;
        $_u_text = $text;
        throw new ImagickDrawException("ImagickDraw::annotation() is not supported in elephc");
    }
    public function arc(float $start_x, float $start_y, float $end_x, float $end_y, float $start_angle, float $end_angle): bool {
        $_u_start_x = $start_x;
        $_u_start_y = $start_y;
        $_u_end_x = $end_x;
        $_u_end_y = $end_y;
        $_u_start_angle = $start_angle;
        $_u_end_angle = $end_angle;
        throw new ImagickDrawException("ImagickDraw::arc() is not supported in elephc");
    }
    public function bezier(array $coordinates): bool {
        $_u_coordinates = $coordinates;
        throw new ImagickDrawException("ImagickDraw::bezier() is not supported in elephc");
    }
    public function clone(): ImagickDraw {
        throw new ImagickDrawException("ImagickDraw::clone() is not supported in elephc");
    }
    public function color(float $x, float $y, int $paint): bool {
        $_u_x = $x;
        $_u_y = $y;
        $_u_paint = $paint;
        throw new ImagickDrawException("ImagickDraw::color() is not supported in elephc");
    }
    public function comment(string $comment): bool {
        $_u_comment = $comment;
        throw new ImagickDrawException("ImagickDraw::comment() is not supported in elephc");
    }
    public function composite(int $composite, float $x, float $y, float $width, float $height, Imagick $image): bool {
        $_u_composite = $composite;
        $_u_x = $x;
        $_u_y = $y;
        $_u_width = $width;
        $_u_height = $height;
        $_u_image = $image;
        throw new ImagickDrawException("ImagickDraw::composite() is not supported in elephc");
    }
    public function getClipPath(): string {
        throw new ImagickDrawException("ImagickDraw::getClipPath() is not supported in elephc");
    }
    public function getClipRule(): int {
        throw new ImagickDrawException("ImagickDraw::getClipRule() is not supported in elephc");
    }
    public function getClipUnits(): int {
        throw new ImagickDrawException("ImagickDraw::getClipUnits() is not supported in elephc");
    }
    public function getFillOpacity(): float {
        throw new ImagickDrawException("ImagickDraw::getFillOpacity() is not supported in elephc");
    }
    public function getFillRule(): int {
        throw new ImagickDrawException("ImagickDraw::getFillRule() is not supported in elephc");
    }
    public function getFont(): string {
        throw new ImagickDrawException("ImagickDraw::getFont() is not supported in elephc");
    }
    public function getFontFamily(): string {
        throw new ImagickDrawException("ImagickDraw::getFontFamily() is not supported in elephc");
    }
    public function getFontSize(): float {
        throw new ImagickDrawException("ImagickDraw::getFontSize() is not supported in elephc");
    }
    public function getFontStretch(): int {
        throw new ImagickDrawException("ImagickDraw::getFontStretch() is not supported in elephc");
    }
    public function getFontStyle(): int {
        throw new ImagickDrawException("ImagickDraw::getFontStyle() is not supported in elephc");
    }
    public function getFontWeight(): int {
        throw new ImagickDrawException("ImagickDraw::getFontWeight() is not supported in elephc");
    }
    public function getGravity(): int {
        throw new ImagickDrawException("ImagickDraw::getGravity() is not supported in elephc");
    }
    public function getStrokeAntialias(): bool {
        throw new ImagickDrawException("ImagickDraw::getStrokeAntialias() is not supported in elephc");
    }
    public function getStrokeColor(): ImagickPixel {
        throw new ImagickDrawException("ImagickDraw::getStrokeColor() is not supported in elephc");
    }
    public function getStrokeDashArray(): array {
        throw new ImagickDrawException("ImagickDraw::getStrokeDashArray() is not supported in elephc");
    }
    public function getStrokeDashOffset(): float {
        throw new ImagickDrawException("ImagickDraw::getStrokeDashOffset() is not supported in elephc");
    }
    public function getStrokeLineCap(): int {
        throw new ImagickDrawException("ImagickDraw::getStrokeLineCap() is not supported in elephc");
    }
    public function getStrokeLineJoin(): int {
        throw new ImagickDrawException("ImagickDraw::getStrokeLineJoin() is not supported in elephc");
    }
    public function getStrokeMiterLimit(): int {
        throw new ImagickDrawException("ImagickDraw::getStrokeMiterLimit() is not supported in elephc");
    }
    public function getStrokeOpacity(): float {
        throw new ImagickDrawException("ImagickDraw::getStrokeOpacity() is not supported in elephc");
    }
    public function getStrokeWidth(): float {
        throw new ImagickDrawException("ImagickDraw::getStrokeWidth() is not supported in elephc");
    }
    public function getTextAlignment(): int {
        throw new ImagickDrawException("ImagickDraw::getTextAlignment() is not supported in elephc");
    }
    public function getTextAntialias(): bool {
        throw new ImagickDrawException("ImagickDraw::getTextAntialias() is not supported in elephc");
    }
    public function getTextDecoration(): int {
        throw new ImagickDrawException("ImagickDraw::getTextDecoration() is not supported in elephc");
    }
    public function getTextEncoding(): string {
        throw new ImagickDrawException("ImagickDraw::getTextEncoding() is not supported in elephc");
    }
    public function getTextInterlineSpacing(): float {
        throw new ImagickDrawException("ImagickDraw::getTextInterlineSpacing() is not supported in elephc");
    }
    public function getTextInterwordSpacing(): float {
        throw new ImagickDrawException("ImagickDraw::getTextInterwordSpacing() is not supported in elephc");
    }
    public function getTextKerning(): float {
        throw new ImagickDrawException("ImagickDraw::getTextKerning() is not supported in elephc");
    }
    public function getTextUnderColor(): ImagickPixel {
        throw new ImagickDrawException("ImagickDraw::getTextUnderColor() is not supported in elephc");
    }
    public function getVectorGraphics(): string {
        throw new ImagickDrawException("ImagickDraw::getVectorGraphics() is not supported in elephc");
    }
    public function matte(float $x, float $y, int $paint): bool {
        $_u_x = $x;
        $_u_y = $y;
        $_u_paint = $paint;
        throw new ImagickDrawException("ImagickDraw::matte() is not supported in elephc");
    }
    public function pathClose(): bool {
        throw new ImagickDrawException("ImagickDraw::pathClose() is not supported in elephc");
    }
    public function pathCurveToAbsolute(float $x1, float $y1, float $x2, float $y2, float $x, float $y): bool {
        $_u_x1 = $x1;
        $_u_y1 = $y1;
        $_u_x2 = $x2;
        $_u_y2 = $y2;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToAbsolute() is not supported in elephc");
    }
    public function pathCurveToQuadraticBezierAbsolute(float $x1, float $y1, float $x_end, float $y): bool {
        $_u_x1 = $x1;
        $_u_y1 = $y1;
        $_u_x_end = $x_end;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToQuadraticBezierAbsolute() is not supported in elephc");
    }
    public function pathCurveToQuadraticBezierRelative(float $x1, float $y1, float $x_end, float $y): bool {
        $_u_x1 = $x1;
        $_u_y1 = $y1;
        $_u_x_end = $x_end;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToQuadraticBezierRelative() is not supported in elephc");
    }
    public function pathCurveToQuadraticBezierSmoothAbsolute(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToQuadraticBezierSmoothAbsolute() is not supported in elephc");
    }
    public function pathCurveToQuadraticBezierSmoothRelative(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToQuadraticBezierSmoothRelative() is not supported in elephc");
    }
    public function pathCurveToRelative(float $x1, float $y1, float $x2, float $y2, float $x, float $y): bool {
        $_u_x1 = $x1;
        $_u_y1 = $y1;
        $_u_x2 = $x2;
        $_u_y2 = $y2;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToRelative() is not supported in elephc");
    }
    public function pathCurveToSmoothAbsolute(float $x2, float $y2, float $x, float $y): bool {
        $_u_x2 = $x2;
        $_u_y2 = $y2;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToSmoothAbsolute() is not supported in elephc");
    }
    public function pathCurveToSmoothRelative(float $x2, float $y2, float $x, float $y): bool {
        $_u_x2 = $x2;
        $_u_y2 = $y2;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathCurveToSmoothRelative() is not supported in elephc");
    }
    public function pathEllipticArcAbsolute(float $rx, float $ry, float $x_axis_rotation, bool $large_arc, bool $sweep, float $x, float $y): bool {
        $_u_rx = $rx;
        $_u_ry = $ry;
        $_u_x_axis_rotation = $x_axis_rotation;
        $_u_large_arc = $large_arc;
        $_u_sweep = $sweep;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathEllipticArcAbsolute() is not supported in elephc");
    }
    public function pathEllipticArcRelative(float $rx, float $ry, float $x_axis_rotation, bool $large_arc, bool $sweep, float $x, float $y): bool {
        $_u_rx = $rx;
        $_u_ry = $ry;
        $_u_x_axis_rotation = $x_axis_rotation;
        $_u_large_arc = $large_arc;
        $_u_sweep = $sweep;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathEllipticArcRelative() is not supported in elephc");
    }
    public function pathFinish(): bool {
        throw new ImagickDrawException("ImagickDraw::pathFinish() is not supported in elephc");
    }
    public function pathLineToAbsolute(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathLineToAbsolute() is not supported in elephc");
    }
    public function pathLineToHorizontalAbsolute(float $x): bool {
        $_u_x = $x;
        throw new ImagickDrawException("ImagickDraw::pathLineToHorizontalAbsolute() is not supported in elephc");
    }
    public function pathLineToHorizontalRelative(float $x): bool {
        $_u_x = $x;
        throw new ImagickDrawException("ImagickDraw::pathLineToHorizontalRelative() is not supported in elephc");
    }
    public function pathLineToRelative(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathLineToRelative() is not supported in elephc");
    }
    public function pathLineToVerticalAbsolute(float $y): bool {
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathLineToVerticalAbsolute() is not supported in elephc");
    }
    public function pathLineToVerticalRelative(float $y): bool {
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathLineToVerticalRelative() is not supported in elephc");
    }
    public function pathMoveToAbsolute(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathMoveToAbsolute() is not supported in elephc");
    }
    public function pathMoveToRelative(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::pathMoveToRelative() is not supported in elephc");
    }
    public function pathStart(): bool {
        throw new ImagickDrawException("ImagickDraw::pathStart() is not supported in elephc");
    }
    public function polyline(array $coordinates): bool {
        $_u_coordinates = $coordinates;
        throw new ImagickDrawException("ImagickDraw::polyline() is not supported in elephc");
    }
    public function pop(): bool {
        throw new ImagickDrawException("ImagickDraw::pop() is not supported in elephc");
    }
    public function popClipPath(): bool {
        throw new ImagickDrawException("ImagickDraw::popClipPath() is not supported in elephc");
    }
    public function popDefs(): bool {
        throw new ImagickDrawException("ImagickDraw::popDefs() is not supported in elephc");
    }
    public function popPattern(): bool {
        throw new ImagickDrawException("ImagickDraw::popPattern() is not supported in elephc");
    }
    public function push(): bool {
        throw new ImagickDrawException("ImagickDraw::push() is not supported in elephc");
    }
    public function pushClipPath(string $clip_mask_id): bool {
        $_u_clip_mask_id = $clip_mask_id;
        throw new ImagickDrawException("ImagickDraw::pushClipPath() is not supported in elephc");
    }
    public function pushDefs(): bool {
        throw new ImagickDrawException("ImagickDraw::pushDefs() is not supported in elephc");
    }
    public function pushPattern(string $pattern_id, float $x, float $y, float $width, float $height): bool {
        $_u_pattern_id = $pattern_id;
        $_u_x = $x;
        $_u_y = $y;
        $_u_width = $width;
        $_u_height = $height;
        throw new ImagickDrawException("ImagickDraw::pushPattern() is not supported in elephc");
    }
    public function render(): bool {
        throw new ImagickDrawException("ImagickDraw::render() is not supported in elephc");
    }
    public function resetVectorGraphics(): bool {
        throw new ImagickDrawException("ImagickDraw::resetVectorGraphics() is not supported in elephc");
    }
    public function rotate(float $degrees): bool {
        $_u_degrees = $degrees;
        throw new ImagickDrawException("ImagickDraw::rotate() is not supported in elephc");
    }
    public function roundRectangle(float $top_left_x, float $top_left_y, float $bottom_right_x, float $bottom_right_y, float $rounding_x, float $rounding_y): bool {
        $_u_top_left_x = $top_left_x;
        $_u_top_left_y = $top_left_y;
        $_u_bottom_right_x = $bottom_right_x;
        $_u_bottom_right_y = $bottom_right_y;
        $_u_rounding_x = $rounding_x;
        $_u_rounding_y = $rounding_y;
        throw new ImagickDrawException("ImagickDraw::roundRectangle() is not supported in elephc");
    }
    public function scale(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::scale() is not supported in elephc");
    }
    public function setClipPath(string $clip_mask): bool {
        $_u_clip_mask = $clip_mask;
        throw new ImagickDrawException("ImagickDraw::setClipPath() is not supported in elephc");
    }
    public function setClipRule(int $fillrule): bool {
        $_u_fillrule = $fillrule;
        throw new ImagickDrawException("ImagickDraw::setClipRule() is not supported in elephc");
    }
    public function setClipUnits(int $pathunits): bool {
        $_u_pathunits = $pathunits;
        throw new ImagickDrawException("ImagickDraw::setClipUnits() is not supported in elephc");
    }
    public function setFillAlpha(float $alpha): bool {
        $_u_alpha = $alpha;
        throw new ImagickDrawException("ImagickDraw::setFillAlpha() is not supported in elephc");
    }
    public function setFillOpacity(float $opacity): bool {
        $_u_opacity = $opacity;
        throw new ImagickDrawException("ImagickDraw::setFillOpacity() is not supported in elephc");
    }
    public function setFillPatternURL(string $fill_url): bool {
        $_u_fill_url = $fill_url;
        throw new ImagickDrawException("ImagickDraw::setFillPatternURL() is not supported in elephc");
    }
    public function setFillRule(int $fillrule): bool {
        $_u_fillrule = $fillrule;
        throw new ImagickDrawException("ImagickDraw::setFillRule() is not supported in elephc");
    }
    public function setFont(string $font_name): bool {
        $_u_font_name = $font_name;
        throw new ImagickDrawException("ImagickDraw::setFont() is not supported in elephc");
    }
    public function setFontFamily(string $font_family): bool {
        $_u_font_family = $font_family;
        throw new ImagickDrawException("ImagickDraw::setFontFamily() is not supported in elephc");
    }
    public function setFontSize(float $point_size): bool {
        $_u_point_size = $point_size;
        throw new ImagickDrawException("ImagickDraw::setFontSize() is not supported in elephc");
    }
    public function setFontStretch(int $stretch): bool {
        $_u_stretch = $stretch;
        throw new ImagickDrawException("ImagickDraw::setFontStretch() is not supported in elephc");
    }
    public function setFontStyle(int $style): bool {
        $_u_style = $style;
        throw new ImagickDrawException("ImagickDraw::setFontStyle() is not supported in elephc");
    }
    public function setFontWeight(int $weight): bool {
        $_u_weight = $weight;
        throw new ImagickDrawException("ImagickDraw::setFontWeight() is not supported in elephc");
    }
    public function setGravity(int $gravity): bool {
        $_u_gravity = $gravity;
        throw new ImagickDrawException("ImagickDraw::setGravity() is not supported in elephc");
    }
    public function setResolution(float $resolution_x, float $resolution_y): bool {
        $_u_resolution_x = $resolution_x;
        $_u_resolution_y = $resolution_y;
        throw new ImagickDrawException("ImagickDraw::setResolution() is not supported in elephc");
    }
    public function setStrokeAlpha(float $alpha): bool {
        $_u_alpha = $alpha;
        throw new ImagickDrawException("ImagickDraw::setStrokeAlpha() is not supported in elephc");
    }
    public function setStrokeAntialias(bool $enabled): bool {
        $_u_enabled = $enabled;
        throw new ImagickDrawException("ImagickDraw::setStrokeAntialias() is not supported in elephc");
    }
    public function setStrokeDashArray(?array $dashes): bool {
        $_u_dashes = $dashes;
        throw new ImagickDrawException("ImagickDraw::setStrokeDashArray() is not supported in elephc");
    }
    public function setStrokeDashOffset(float $dash_offset): bool {
        $_u_dash_offset = $dash_offset;
        throw new ImagickDrawException("ImagickDraw::setStrokeDashOffset() is not supported in elephc");
    }
    public function setStrokeLineCap(int $linecap): bool {
        $_u_linecap = $linecap;
        throw new ImagickDrawException("ImagickDraw::setStrokeLineCap() is not supported in elephc");
    }
    public function setStrokeLineJoin(int $linejoin): bool {
        $_u_linejoin = $linejoin;
        throw new ImagickDrawException("ImagickDraw::setStrokeLineJoin() is not supported in elephc");
    }
    public function setStrokeMiterLimit(int $miterlimit): bool {
        $_u_miterlimit = $miterlimit;
        throw new ImagickDrawException("ImagickDraw::setStrokeMiterLimit() is not supported in elephc");
    }
    public function setStrokeOpacity(float $opacity): bool {
        $_u_opacity = $opacity;
        throw new ImagickDrawException("ImagickDraw::setStrokeOpacity() is not supported in elephc");
    }
    public function setStrokePatternURL(string $stroke_url): bool {
        $_u_stroke_url = $stroke_url;
        throw new ImagickDrawException("ImagickDraw::setStrokePatternURL() is not supported in elephc");
    }
    public function setTextAlignment(int $align): bool {
        $_u_align = $align;
        throw new ImagickDrawException("ImagickDraw::setTextAlignment() is not supported in elephc");
    }
    public function setTextAntialias(bool $antialias): bool {
        $_u_antialias = $antialias;
        throw new ImagickDrawException("ImagickDraw::setTextAntialias() is not supported in elephc");
    }
    public function setTextDecoration(int $decoration): bool {
        $_u_decoration = $decoration;
        throw new ImagickDrawException("ImagickDraw::setTextDecoration() is not supported in elephc");
    }
    public function setTextEncoding(string $encoding): bool {
        $_u_encoding = $encoding;
        throw new ImagickDrawException("ImagickDraw::setTextEncoding() is not supported in elephc");
    }
    public function setTextInterlineSpacing(float $spacing): bool {
        $_u_spacing = $spacing;
        throw new ImagickDrawException("ImagickDraw::setTextInterlineSpacing() is not supported in elephc");
    }
    public function setTextInterwordSpacing(float $spacing): bool {
        $_u_spacing = $spacing;
        throw new ImagickDrawException("ImagickDraw::setTextInterwordSpacing() is not supported in elephc");
    }
    public function setTextKerning(float $kerning): bool {
        $_u_kerning = $kerning;
        throw new ImagickDrawException("ImagickDraw::setTextKerning() is not supported in elephc");
    }
    public function setTextUnderColor(ImagickPixel|string $under_color): bool {
        $_u_under_color = $under_color;
        throw new ImagickDrawException("ImagickDraw::setTextUnderColor() is not supported in elephc");
    }
    public function setVectorGraphics(string $xml): bool {
        $_u_xml = $xml;
        throw new ImagickDrawException("ImagickDraw::setVectorGraphics() is not supported in elephc");
    }
    public function setViewbox(int $left_x, int $top_y, int $right_x, int $bottom_y): bool {
        $_u_left_x = $left_x;
        $_u_top_y = $top_y;
        $_u_right_x = $right_x;
        $_u_bottom_y = $bottom_y;
        throw new ImagickDrawException("ImagickDraw::setViewbox() is not supported in elephc");
    }
    public function skewX(float $degrees): bool {
        $_u_degrees = $degrees;
        throw new ImagickDrawException("ImagickDraw::skewX() is not supported in elephc");
    }
    public function skewY(float $degrees): bool {
        $_u_degrees = $degrees;
        throw new ImagickDrawException("ImagickDraw::skewY() is not supported in elephc");
    }
    public function translate(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickDrawException("ImagickDraw::translate() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---

}

class Imagick implements Iterator, Countable {
    // Resize filter constants (accepted for API parity; elephc resizes bilinear).
    const FILTER_UNDEFINED = 0;
    const FILTER_POINT = 1;
    const FILTER_BOX = 2;
    const FILTER_TRIANGLE = 3;
    const FILTER_HERMITE = 4;
    const FILTER_HANNING = 5;
    const FILTER_HAMMING = 6;
    const FILTER_BLACKMAN = 7;
    const FILTER_GAUSSIAN = 8;
    const FILTER_QUADRATIC = 9;
    const FILTER_CUBIC = 10;
    const FILTER_CATROM = 11;
    const FILTER_MITCHELL = 12;
    const FILTER_LANCZOS = 22;
    const FILTER_SINC = 19;
    // Composite operators. Only OVER/COPY are implemented; others throw.
    const COMPOSITE_DEFAULT = 40;
    const COMPOSITE_OVER = 40;
    const COMPOSITE_COPY = 42;
    const COMPOSITE_MULTIPLY = 30;
    const COMPOSITE_SCREEN = 46;
    const COMPOSITE_ADD = 7;
    // Channel constants.
    const CHANNEL_RED = 1;
    const CHANNEL_GREEN = 2;
    const CHANNEL_BLUE = 4;
    const CHANNEL_ALPHA = 8;
    const CHANNEL_OPACITY = 8;
    const CHANNEL_ALL = 134217727;
    const CHANNEL_DEFAULT = 134217719;
    // ImagickPixel color-channel selectors (Imagick::COLOR_*).
    const COLOR_BLACK = 0;
    const COLOR_BLUE = 1;
    const COLOR_GREEN = 3;
    const COLOR_RED = 4;
    const COLOR_OPACITY = 7;
    const COLOR_ALPHA = 8;
    // Image type constants (subset).
    const IMGTYPE_UNDEFINED = 0;
    const IMGTYPE_GRAYSCALE = 2;
    const IMGTYPE_PALETTE = 3;
    const IMGTYPE_TRUECOLOR = 6;
    // Orientation (subset).
    const ORIENTATION_UNDEFINED = 0;
    const ORIENTATION_TOPLEFT = 1;

    private int $wand = 0;
    private int $_iterPos = 0;

    public function __construct(?string $files = null) {
        $this->wand = elephc_imagick_new();
        if ($files !== null && $files !== "") {
            $this->readImage((string) $files);
        }
    }

    // Internal: exposes the bridge wand handle to sibling Imagick objects and the
    // pixel iterator.
    public function _wandHandle(): int {
        return $this->wand;
    }

    public function readImage(string $filename): bool {
        if (elephc_imagick_read_file($this->wand, $filename) !== 0) {
            throw new ImagickException("Imagick::readImage(): unable to read '" . $filename . "'");
        }
        return true;
    }

    public function readImageBlob(string $image, string $filename = ""): bool {
        $_u_name = $filename;
        $_len = strlen($image);
        if ($_len <= 0) {
            throw new ImagickException("Imagick::readImageBlob(): empty blob");
        }
        $_buf = elephc_img_stage_ptr($_len);
        if (ptr_is_null($_buf)) {
            throw new ImagickException("Imagick::readImageBlob(): allocation failed");
        }
        ptr_write_string($_buf, $image);
        if (elephc_imagick_read_blob($this->wand, $_len) !== 0) {
            throw new ImagickException("Imagick::readImageBlob(): unrecognized image data");
        }
        return true;
    }

    public function newImage(int $columns, int $rows, $background, string $format = ""): bool {
        $_bg = _imagick_norm_color($background);
        $_fmt = $format === "" ? 0 : _imagick_fmt_to_code($format);
        if (elephc_imagick_new_image($this->wand, $columns, $rows, $_bg, $_fmt) !== 0) {
            throw new ImagickException("Imagick::newImage(): invalid dimensions");
        }
        return true;
    }

    public function addImage(Imagick $source): bool {
        if (elephc_imagick_add_image($this->wand, $source->_wandHandle()) !== 0) {
            throw new ImagickException("Imagick::addImage(): no source image");
        }
        return true;
    }

    public function writeImage(?string $filename = null): bool {
        if ($filename === null || $filename === "") {
            throw new ImagickException("Imagick::writeImage(): no filename given");
        }
        $_path = (string) $filename;
        $_fmt = _imagick_fmt_from_path($_path);
        if (elephc_imagick_write_file($this->wand, $_path, $_fmt) !== 0) {
            throw new ImagickException("Imagick::writeImage(): unable to write '" . $_path . "'");
        }
        return true;
    }

    public function writeImages(string $filename, bool $adjoin): bool {
        $_u_adjoin = $adjoin;
        return $this->writeImage($filename);
    }

    public function getImageBlob(): string {
        $_len = elephc_imagick_get_blob($this->wand, 0);
        if ($_len < 0) {
            throw new ImagickException("Imagick::getImageBlob(): no image or encode failed");
        }
        $_bytes = ptr_read_string(elephc_img_encoded_ptr(), $_len);
        elephc_img_encoded_clear();
        return $_bytes;
    }

    public function getImagesBlob(): string {
        return $this->getImageBlob();
    }

    public function setImageFormat(string $format): bool {
        $_code = _imagick_fmt_to_code($format);
        if ($_code === 0) {
            throw new ImagickException("Imagick::setImageFormat(): unsupported format '" . $format . "'");
        }
        elephc_imagick_set_format($this->wand, $_code);
        return true;
    }

    public function getImageFormat(): string {
        return _imagick_code_to_fmt(elephc_imagick_get_format($this->wand));
    }

    public function setFormat(string $format): bool {
        return $this->setImageFormat($format);
    }

    public function getFormat(): string {
        return $this->getImageFormat();
    }

    public function setImageCompressionQuality(int $quality): bool {
        elephc_imagick_set_quality($this->wand, $quality);
        return true;
    }

    public function getImageCompressionQuality(): int {
        $_q = elephc_imagick_get_quality($this->wand);
        return $_q < 0 ? 0 : $_q;
    }

    public function setCompressionQuality(int $quality): bool {
        return $this->setImageCompressionQuality($quality);
    }

    public function getImageWidth(): int {
        return elephc_imagick_cur_width($this->wand);
    }

    public function getImageHeight(): int {
        return elephc_imagick_cur_height($this->wand);
    }

    // No return type hint so callers can read the "width"/"height" keys.
    public function getImageGeometry() {
        return ["width" => $this->getImageWidth(), "height" => $this->getImageHeight()];
    }

    // Returns [width, height] honoring the aspect ratio within the box.
    private function _bestfit(int $cols, int $rows): array {
        $_ow = $this->getImageWidth();
        $_oh = $this->getImageHeight();
        if ($_ow <= 0 || $_oh <= 0 || $cols <= 0 || $rows <= 0) {
            return [$cols, $rows];
        }
        $_rw = $cols / $_ow;
        $_rh = $rows / $_oh;
        $_ratio = $_rw < $_rh ? $_rw : $_rh;
        return [(int) round($_ow * $_ratio), (int) round($_oh * $_ratio)];
    }

    public function resizeImage(int $columns, int $rows, int $filter, float $blur, bool $bestfit = false): bool {
        $_u_filter = $filter;
        $_u_blur = $blur;
        $_d = $bestfit ? $this->_bestfit($columns, $rows) : [$columns, $rows];
        if (elephc_imagick_resize($this->wand, $_d[0], $_d[1]) !== 0) {
            throw new ImagickException("Imagick::resizeImage(): resize failed");
        }
        return true;
    }

    public function scaleImage(int $columns, int $rows, bool $bestfit = false): bool {
        $_d = $bestfit ? $this->_bestfit($columns, $rows) : [$columns, $rows];
        if (elephc_imagick_scale($this->wand, $_d[0], $_d[1]) !== 0) {
            throw new ImagickException("Imagick::scaleImage(): scale failed");
        }
        return true;
    }

    public function thumbnailImage(int $columns, int $rows, bool $bestfit = false, bool $fill = false): bool {
        $_u_fill = $fill;
        $_ow = $this->getImageWidth();
        $_oh = $this->getImageHeight();
        $_w = $columns;
        $_h = $rows;
        if ($columns == 0 && $rows > 0 && $_oh > 0) {
            $_w = (int) round($_ow * $rows / $_oh);
        } elseif ($rows == 0 && $columns > 0 && $_ow > 0) {
            $_h = (int) round($_oh * $columns / $_ow);
        } elseif ($bestfit && $columns > 0 && $rows > 0) {
            $_d = $this->_bestfit($columns, $rows);
            $_w = $_d[0];
            $_h = $_d[1];
        }
        if ($_w < 1) {
            $_w = 1;
        }
        if ($_h < 1) {
            $_h = 1;
        }
        if (elephc_imagick_resize($this->wand, $_w, $_h) !== 0) {
            throw new ImagickException("Imagick::thumbnailImage(): failed");
        }
        return true;
    }

    public function cropImage(int $width, int $height, int $x, int $y): bool {
        if (elephc_imagick_crop($this->wand, $width, $height, $x, $y) !== 0) {
            throw new ImagickException("Imagick::cropImage(): invalid crop region");
        }
        return true;
    }

    public function rotateImage($background, float $degrees): bool {
        $_bg = _imagick_norm_color($background);
        $_mdeg = (int) round($degrees * 1000);
        if (elephc_imagick_rotate($this->wand, $_mdeg, $_bg) !== 0) {
            throw new ImagickException("Imagick::rotateImage(): rotate failed");
        }
        return true;
    }

    public function flipImage(): bool {
        if (elephc_imagick_flip($this->wand) !== 0) {
            throw new ImagickException("Imagick::flipImage(): no image");
        }
        return true;
    }

    public function flopImage(): bool {
        if (elephc_imagick_flop($this->wand) !== 0) {
            throw new ImagickException("Imagick::flopImage(): no image");
        }
        return true;
    }

    public function blurImage(float $radius, float $sigma): bool {
        $_sig = $sigma > 0.0 ? $sigma : $radius;
        if (elephc_imagick_blur($this->wand, (int) round($_sig * 1000)) !== 0) {
            throw new ImagickException("Imagick::blurImage(): no image");
        }
        return true;
    }

    public function gaussianBlurImage(float $radius, float $sigma): bool {
        return $this->blurImage($radius, $sigma);
    }

    public function negateImage(bool $gray = false): bool {
        if (elephc_imagick_negate($this->wand, $gray ? 1 : 0) !== 0) {
            throw new ImagickException("Imagick::negateImage(): no image");
        }
        return true;
    }

    public function modulateImage($brightness, $saturation, $hue): bool {
        $_b = (int) round($brightness);
        $_s = (int) round($saturation);
        $_h = (int) round($hue);
        if (elephc_imagick_modulate($this->wand, $_b, $_s, $_h) !== 0) {
            throw new ImagickException("Imagick::modulateImage(): no image");
        }
        return true;
    }

    public function sharpenImage($radius, $sigma): bool {
        $_r = (int) round(((float) $radius) * 1000);
        $_s = (int) round(((float) $sigma) * 1000);
        if (elephc_imagick_sharpen($this->wand, $_r, $_s) !== 0) {
            throw new ImagickException("Imagick::sharpenImage(): no image");
        }
        return true;
    }

    public function compositeImage(Imagick $composite, int $composite_op, int $x, int $y): bool {
        $_rc = elephc_imagick_composite($this->wand, $composite->_wandHandle(), $composite_op, $x, $y);
        if ($_rc === -2) {
            throw new ImagickException("Imagick::compositeImage(): composite operator " . $composite_op . " is not supported in elephc");
        }
        if ($_rc !== 0) {
            throw new ImagickException("Imagick::compositeImage(): composite failed");
        }
        return true;
    }

    public function drawImage(ImagickDraw $draw): bool {
        if (elephc_imagick_draw($this->wand, $draw->_imagickHandle()) !== 0) {
            throw new ImagickException("Imagick::drawImage(): draw failed");
        }
        return true;
    }

    public function convolveImage(ImagickKernel $kernel): bool {
        if ($kernel->_size() !== 3) {
            throw new ImagickException("Imagick::convolveImage(): only 3x3 kernels are supported in elephc");
        }
        elephc_img_fbuf_reset();
        for ($_i = 0; $_i < 9; $_i++) {
            elephc_img_fbuf_push((int) round($kernel->_at($_i) * 65536));
        }
        $_div = (int) round($kernel->_divisor() * 65536);
        if (elephc_imagick_convolve($this->wand, $_div, 0) !== 0) {
            throw new ImagickException("Imagick::convolveImage(): convolution failed");
        }
        return true;
    }

    public function getImagePixelColor(int $x, int $y): ImagickPixel {
        $_packed = elephc_imagick_pixel_color($this->wand, $x, $y);
        if ($_packed < 0) {
            throw new ImagickException("Imagick::getImagePixelColor(): coordinate out of range");
        }
        return _imagick_pixel_from_int($_packed);
    }

    // elephc paints the background color onto the current frame (it has no
    // deferred background slot); documented difference from ImageMagick.
    public function setImageBackgroundColor($background): bool {
        elephc_imagick_fill($this->wand, _imagick_norm_color($background));
        return true;
    }

    public function getPixelIterator(): ImagickPixelIterator {
        return new ImagickPixelIterator($this);
    }

    public function getNumberImages(): int {
        return elephc_imagick_count($this->wand);
    }

    public function getImageIndex(): int {
        return elephc_imagick_get_index($this->wand);
    }

    public function setImageIndex(int $index): bool {
        return elephc_imagick_set_index($this->wand, $index) === 0;
    }

    public function getIteratorIndex(): int {
        return elephc_imagick_get_index($this->wand);
    }

    public function setIteratorIndex(int $index): bool {
        return elephc_imagick_set_index($this->wand, $index) === 0;
    }

    public function nextImage(): bool {
        return elephc_imagick_next($this->wand) === 1;
    }

    public function previousImage(): bool {
        return elephc_imagick_previous($this->wand) === 1;
    }

    public function setFirstIterator(): bool {
        elephc_imagick_first($this->wand);
        return true;
    }

    public function setLastIterator(): bool {
        elephc_imagick_last($this->wand);
        return true;
    }

    // Countable: count($imagick) returns the number of frames.
    public function count(): int {
        return elephc_imagick_count($this->wand);
    }

    // Iterator: foreach over the wand positions the wand at each frame and yields
    // the wand itself, matching PHP's Imagick Traversable behavior.
    public function rewind(): void {
        $this->_iterPos = 0;
        elephc_imagick_set_index($this->wand, 0);
    }

    public function valid(): bool {
        return $this->_iterPos < elephc_imagick_count($this->wand);
    }

    public function current(): mixed {
        elephc_imagick_set_index($this->wand, $this->_iterPos);
        return $this;
    }

    public function key(): mixed {
        return $this->_iterPos;
    }

    public function next(): void {
        $this->_iterPos = $this->_iterPos + 1;
    }

    public function clear(): bool {
        elephc_imagick_clear($this->wand);
        return true;
    }

    public function destroy(): bool {
        elephc_imagick_destroy($this->wand);
        $this->wand = 0;
        return true;
    }

    public function __destruct() {
        elephc_imagick_destroy($this->wand);
    }

    // Returns the formats the pure-Rust codec bridge can read/write.
    public static function queryFormats(string $pattern = "*"): array {
        $_u_pat = $pattern;
        return ["BMP", "GIF", "JPEG", "PNG", "WEBP"];
    }

    // -- Documented unsupported operators (no pure-Rust equivalent) --
    public function distortImage(int $method, array $arguments, bool $bestfit): bool {
        $_u_m = $method;
        $_u_a = $arguments;
        $_u_b = $bestfit;
        throw new ImagickException("Imagick::distortImage() is not supported in elephc");
    }

    public function liquidRescaleImage(int $width, int $height, float $delta_x, float $rigidity): bool {
        $_u_w = $width;
        $_u_h = $height;
        $_u_dx = $delta_x;
        $_u_r = $rigidity;
        throw new ImagickException("Imagick::liquidRescaleImage() is not supported in elephc");
    }

    public function fxImage(string $expression): Imagick {
        $_u_e = $expression;
        throw new ImagickException("Imagick::fxImage() is not supported in elephc");
    }

    public function annotateImage(ImagickDraw $draw, float $x, float $y, float $angle, string $text): bool {
        $_u_d = $draw;
        $_u_x = $x;
        $_u_y = $y;
        $_u_a = $angle;
        $_u_t = $text;
        throw new ImagickException("Imagick::annotateImage() requires FreeType text, which is not supported in elephc");
    }

    public function waveImage(float $amplitude, float $length): bool {
        $_u_a = $amplitude;
        $_u_l = $length;
        throw new ImagickException("Imagick::waveImage() is not supported in elephc");
    }

    public function swirlImage(float $degrees): bool {
        $_u_d = $degrees;
        throw new ImagickException("Imagick::swirlImage() is not supported in elephc");
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function adaptiveBlurImage(float $radius, float $sigma, int $channel = 0): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::adaptiveBlurImage() is not supported in elephc");
    }
    public function adaptiveResizeImage(int $columns, int $rows, bool $bestfit = false, bool $legacy = false): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        $_u_bestfit = $bestfit;
        $_u_legacy = $legacy;
        throw new ImagickException("Imagick::adaptiveResizeImage() is not supported in elephc");
    }
    public function adaptiveSharpenImage(float $radius, float $sigma, int $channel = 0): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::adaptiveSharpenImage() is not supported in elephc");
    }
    public function adaptiveThresholdImage(int $width, int $height, int $offset): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_offset = $offset;
        throw new ImagickException("Imagick::adaptiveThresholdImage() is not supported in elephc");
    }
    public function addNoiseImage(int $noise_type, int $channel = 0): bool {
        $_u_noise_type = $noise_type;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::addNoiseImage() is not supported in elephc");
    }
    public function affineTransformImage(ImagickDraw $matrix): bool {
        $_u_matrix = $matrix;
        throw new ImagickException("Imagick::affineTransformImage() is not supported in elephc");
    }
    public function animateImages(string $x_server): bool {
        $_u_x_server = $x_server;
        throw new ImagickException("Imagick::animateImages() is not supported in elephc");
    }
    public function appendImages(bool $stack): Imagick {
        $_u_stack = $stack;
        throw new ImagickException("Imagick::appendImages() is not supported in elephc");
    }
    public function autoLevelImage(int $channel = 0): bool {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::autoLevelImage() is not supported in elephc");
    }
    public function averageImages(): Imagick {
        throw new ImagickException("Imagick::averageImages() is not supported in elephc");
    }
    public function blackThresholdImage($threshold): bool {
        $_u_threshold = $threshold;
        throw new ImagickException("Imagick::blackThresholdImage() is not supported in elephc");
    }
    public function blueShiftImage(float $factor = 1.5): bool {
        $_u_factor = $factor;
        throw new ImagickException("Imagick::blueShiftImage() is not supported in elephc");
    }
    public function borderImage($bordercolor, int $width, int $height): bool {
        $_u_bordercolor = $bordercolor;
        $_u_width = $width;
        $_u_height = $height;
        throw new ImagickException("Imagick::borderImage() is not supported in elephc");
    }
    public function brightnessContrastImage(float $brightness, float $contrast, int $channel = 0): bool {
        $_u_brightness = $brightness;
        $_u_contrast = $contrast;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::brightnessContrastImage() is not supported in elephc");
    }
    public function charcoalImage(float $radius, float $sigma): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        throw new ImagickException("Imagick::charcoalImage() is not supported in elephc");
    }
    public function chopImage(int $width, int $height, int $x, int $y): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::chopImage() is not supported in elephc");
    }
    public function clampImage(int $channel = 0): bool {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::clampImage() is not supported in elephc");
    }
    public function clipImage(): bool {
        throw new ImagickException("Imagick::clipImage() is not supported in elephc");
    }
    public function clipImagePath(string $pathname, string $inside): void {
        $_u_pathname = $pathname;
        $_u_inside = $inside;
        throw new ImagickException("Imagick::clipImagePath() is not supported in elephc");
    }
    public function clipPathImage(string $pathname, bool $inside): bool {
        $_u_pathname = $pathname;
        $_u_inside = $inside;
        throw new ImagickException("Imagick::clipPathImage() is not supported in elephc");
    }
    public function clone(): Imagick {
        throw new ImagickException("Imagick::clone() is not supported in elephc");
    }
    public function clutImage(Imagick $lookup_table, int $channel = 0): bool {
        $_u_lookup_table = $lookup_table;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::clutImage() is not supported in elephc");
    }
    public function coalesceImages(): Imagick {
        throw new ImagickException("Imagick::coalesceImages() is not supported in elephc");
    }
    public function colorFloodfillImage($fill, float $fuzz, $bordercolor, int $x, int $y): bool {
        $_u_fill = $fill;
        $_u_fuzz = $fuzz;
        $_u_bordercolor = $bordercolor;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::colorFloodfillImage() is not supported in elephc");
    }
    public function colorizeImage($colorize, $opacity, bool $legacy = false): bool {
        $_u_colorize = $colorize;
        $_u_opacity = $opacity;
        $_u_legacy = $legacy;
        throw new ImagickException("Imagick::colorizeImage() is not supported in elephc");
    }
    public function colorMatrixImage(array $color_matrix): bool {
        $_u_color_matrix = $color_matrix;
        throw new ImagickException("Imagick::colorMatrixImage() is not supported in elephc");
    }
    public function combineImages(int $channelType): Imagick {
        $_u_channelType = $channelType;
        throw new ImagickException("Imagick::combineImages() is not supported in elephc");
    }
    public function commentImage(string $comment): bool {
        $_u_comment = $comment;
        throw new ImagickException("Imagick::commentImage() is not supported in elephc");
    }
    public function compareImageChannels(Imagick $image, int $channelType, int $metricType): array {
        $_u_image = $image;
        $_u_channelType = $channelType;
        $_u_metricType = $metricType;
        throw new ImagickException("Imagick::compareImageChannels() is not supported in elephc");
    }
    public function compareImageLayers(int $method): Imagick {
        $_u_method = $method;
        throw new ImagickException("Imagick::compareImageLayers() is not supported in elephc");
    }
    public function compareImages(Imagick $compare, int $metric): array {
        $_u_compare = $compare;
        $_u_metric = $metric;
        throw new ImagickException("Imagick::compareImages() is not supported in elephc");
    }
    public function contrastImage(bool $sharpen): bool {
        $_u_sharpen = $sharpen;
        throw new ImagickException("Imagick::contrastImage() is not supported in elephc");
    }
    public function contrastStretchImage(float $black_point, float $white_point, int $channel = 0): bool {
        $_u_black_point = $black_point;
        $_u_white_point = $white_point;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::contrastStretchImage() is not supported in elephc");
    }
    public function cropThumbnailImage(int $width, int $height, bool $legacy = false): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_legacy = $legacy;
        throw new ImagickException("Imagick::cropThumbnailImage() is not supported in elephc");
    }
    public function cycleColormapImage(int $displace): bool {
        $_u_displace = $displace;
        throw new ImagickException("Imagick::cycleColormapImage() is not supported in elephc");
    }
    public function decipherImage(string $passphrase): bool {
        $_u_passphrase = $passphrase;
        throw new ImagickException("Imagick::decipherImage() is not supported in elephc");
    }
    public function deconstructImages(): Imagick {
        throw new ImagickException("Imagick::deconstructImages() is not supported in elephc");
    }
    public function deleteImageArtifact(string $artifact): bool {
        $_u_artifact = $artifact;
        throw new ImagickException("Imagick::deleteImageArtifact() is not supported in elephc");
    }
    public function deleteImageProperty(string $name): bool {
        $_u_name = $name;
        throw new ImagickException("Imagick::deleteImageProperty() is not supported in elephc");
    }
    public function deskewImage(float $threshold): bool {
        $_u_threshold = $threshold;
        throw new ImagickException("Imagick::deskewImage() is not supported in elephc");
    }
    public function despeckleImage(): bool {
        throw new ImagickException("Imagick::despeckleImage() is not supported in elephc");
    }
    public function displayImage(string $servername): bool {
        $_u_servername = $servername;
        throw new ImagickException("Imagick::displayImage() is not supported in elephc");
    }
    public function displayImages(string $servername): bool {
        $_u_servername = $servername;
        throw new ImagickException("Imagick::displayImages() is not supported in elephc");
    }
    public function edgeImage(float $radius): bool {
        $_u_radius = $radius;
        throw new ImagickException("Imagick::edgeImage() is not supported in elephc");
    }
    public function embossImage(float $radius, float $sigma): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        throw new ImagickException("Imagick::embossImage() is not supported in elephc");
    }
    public function encipherImage(string $passphrase): bool {
        $_u_passphrase = $passphrase;
        throw new ImagickException("Imagick::encipherImage() is not supported in elephc");
    }
    public function enhanceImage(): bool {
        throw new ImagickException("Imagick::enhanceImage() is not supported in elephc");
    }
    public function equalizeImage(): bool {
        throw new ImagickException("Imagick::equalizeImage() is not supported in elephc");
    }
    public function evaluateImage(int $op, float $constant, int $channel = 0): bool {
        $_u_op = $op;
        $_u_constant = $constant;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::evaluateImage() is not supported in elephc");
    }
    public function exportImagePixels(int $x, int $y, int $width, int $height, string $map, int $sTORAGE): array {
        $_u_x = $x;
        $_u_y = $y;
        $_u_width = $width;
        $_u_height = $height;
        $_u_map = $map;
        $_u_sTORAGE = $sTORAGE;
        throw new ImagickException("Imagick::exportImagePixels() is not supported in elephc");
    }
    public function extentImage(int $width, int $height, int $x, int $y): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::extentImage() is not supported in elephc");
    }
    public function filter(ImagickKernel $imagickKernel, int $channel = 0): bool {
        $_u_imagickKernel = $imagickKernel;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::filter() is not supported in elephc");
    }
    public function flattenImages(): Imagick {
        throw new ImagickException("Imagick::flattenImages() is not supported in elephc");
    }
    public function floodFillPaintImage($fill, float $fuzz, $target, int $x, int $y, bool $invert, int $channel = 0): bool {
        $_u_fill = $fill;
        $_u_fuzz = $fuzz;
        $_u_target = $target;
        $_u_x = $x;
        $_u_y = $y;
        $_u_invert = $invert;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::floodFillPaintImage() is not supported in elephc");
    }
    public function forwardFourierTransformimage(bool $magnitude): bool {
        $_u_magnitude = $magnitude;
        throw new ImagickException("Imagick::forwardFourierTransformimage() is not supported in elephc");
    }
    public function frameImage($matte_color, int $width, int $height, int $inner_bevel, int $outer_bevel): bool {
        $_u_matte_color = $matte_color;
        $_u_width = $width;
        $_u_height = $height;
        $_u_inner_bevel = $inner_bevel;
        $_u_outer_bevel = $outer_bevel;
        throw new ImagickException("Imagick::frameImage() is not supported in elephc");
    }
    public function functionImage(int $function, array $arguments, int $channel = 0): bool {
        $_u_function = $function;
        $_u_arguments = $arguments;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::functionImage() is not supported in elephc");
    }
    public function gammaImage(float $gamma, int $channel = 0): bool {
        $_u_gamma = $gamma;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::gammaImage() is not supported in elephc");
    }
    public function getColorspace(): int {
        throw new ImagickException("Imagick::getColorspace() is not supported in elephc");
    }
    public function getCompression(): int {
        throw new ImagickException("Imagick::getCompression() is not supported in elephc");
    }
    public function getCompressionQuality(): int {
        throw new ImagickException("Imagick::getCompressionQuality() is not supported in elephc");
    }
    public static function getCopyright(): string {
        throw new ImagickException("Imagick::getCopyright() is not supported in elephc");
    }
    public function getFilename(): string {
        throw new ImagickException("Imagick::getFilename() is not supported in elephc");
    }
    public function getFont(): string {
        throw new ImagickException("Imagick::getFont() is not supported in elephc");
    }
    public function getGravity(): int {
        throw new ImagickException("Imagick::getGravity() is not supported in elephc");
    }
    public static function getHomeURL(): string {
        throw new ImagickException("Imagick::getHomeURL() is not supported in elephc");
    }
    public function getImage(): Imagick {
        throw new ImagickException("Imagick::getImage() is not supported in elephc");
    }
    public function getImageAlphaChannel(): bool {
        throw new ImagickException("Imagick::getImageAlphaChannel() is not supported in elephc");
    }
    public function getImageArtifact(string $artifact): string {
        $_u_artifact = $artifact;
        throw new ImagickException("Imagick::getImageArtifact() is not supported in elephc");
    }
    public function getImageAttribute(string $key): string {
        $_u_key = $key;
        throw new ImagickException("Imagick::getImageAttribute() is not supported in elephc");
    }
    public function getImageBackgroundColor(): ImagickPixel {
        throw new ImagickException("Imagick::getImageBackgroundColor() is not supported in elephc");
    }
    public function getImageBluePrimary(): array {
        throw new ImagickException("Imagick::getImageBluePrimary() is not supported in elephc");
    }
    public function getImageBorderColor(): ImagickPixel {
        throw new ImagickException("Imagick::getImageBorderColor() is not supported in elephc");
    }
    public function getImageChannelDepth(int $channel): int {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::getImageChannelDepth() is not supported in elephc");
    }
    public function getImageChannelDistortion(Imagick $reference, int $channel, int $metric): float {
        $_u_reference = $reference;
        $_u_channel = $channel;
        $_u_metric = $metric;
        throw new ImagickException("Imagick::getImageChannelDistortion() is not supported in elephc");
    }
    public function getImageChannelDistortions(Imagick $reference, int $metric, int $channel = 0): float {
        $_u_reference = $reference;
        $_u_metric = $metric;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::getImageChannelDistortions() is not supported in elephc");
    }
    public function getImageChannelExtrema(int $channel): array {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::getImageChannelExtrema() is not supported in elephc");
    }
    public function getImageChannelKurtosis(int $channel = 0): array {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::getImageChannelKurtosis() is not supported in elephc");
    }
    public function getImageChannelMean(int $channel): array {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::getImageChannelMean() is not supported in elephc");
    }
    public function getImageChannelRange(int $channel): array {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::getImageChannelRange() is not supported in elephc");
    }
    public function getImageChannelStatistics(): array {
        throw new ImagickException("Imagick::getImageChannelStatistics() is not supported in elephc");
    }
    public function getImageClipMask(): Imagick {
        throw new ImagickException("Imagick::getImageClipMask() is not supported in elephc");
    }
    public function getImageColormapColor(int $index): ImagickPixel {
        $_u_index = $index;
        throw new ImagickException("Imagick::getImageColormapColor() is not supported in elephc");
    }
    public function getImageColors(): int {
        throw new ImagickException("Imagick::getImageColors() is not supported in elephc");
    }
    public function getImageColorspace(): int {
        throw new ImagickException("Imagick::getImageColorspace() is not supported in elephc");
    }
    public function getImageCompose(): int {
        throw new ImagickException("Imagick::getImageCompose() is not supported in elephc");
    }
    public function getImageCompression(): int {
        throw new ImagickException("Imagick::getImageCompression() is not supported in elephc");
    }
    public function getImageDelay(): int {
        throw new ImagickException("Imagick::getImageDelay() is not supported in elephc");
    }
    public function getImageDepth(): int {
        throw new ImagickException("Imagick::getImageDepth() is not supported in elephc");
    }
    public function getImageDispose(): int {
        throw new ImagickException("Imagick::getImageDispose() is not supported in elephc");
    }
    public function getImageDistortion(Imagick $reference, int $metric): float {
        $_u_reference = $reference;
        $_u_metric = $metric;
        throw new ImagickException("Imagick::getImageDistortion() is not supported in elephc");
    }
    public function getImageExtrema(): array {
        throw new ImagickException("Imagick::getImageExtrema() is not supported in elephc");
    }
    public function getImageFilename(): string {
        throw new ImagickException("Imagick::getImageFilename() is not supported in elephc");
    }
    public function getImageGamma(): float {
        throw new ImagickException("Imagick::getImageGamma() is not supported in elephc");
    }
    public function getImageGravity(): int {
        throw new ImagickException("Imagick::getImageGravity() is not supported in elephc");
    }
    public function getImageGreenPrimary(): array {
        throw new ImagickException("Imagick::getImageGreenPrimary() is not supported in elephc");
    }
    public function getImageHistogram(): array {
        throw new ImagickException("Imagick::getImageHistogram() is not supported in elephc");
    }
    public function getImageInterlaceScheme(): int {
        throw new ImagickException("Imagick::getImageInterlaceScheme() is not supported in elephc");
    }
    public function getImageInterpolateMethod(): int {
        throw new ImagickException("Imagick::getImageInterpolateMethod() is not supported in elephc");
    }
    public function getImageIterations(): int {
        throw new ImagickException("Imagick::getImageIterations() is not supported in elephc");
    }
    public function getImageLength(): int {
        throw new ImagickException("Imagick::getImageLength() is not supported in elephc");
    }
    public function getImageMatte(): bool {
        throw new ImagickException("Imagick::getImageMatte() is not supported in elephc");
    }
    public function getImageMatteColor(): ImagickPixel {
        throw new ImagickException("Imagick::getImageMatteColor() is not supported in elephc");
    }
    public function getImageMimeType(): string {
        throw new ImagickException("Imagick::getImageMimeType() is not supported in elephc");
    }
    public function getImageOrientation(): int {
        throw new ImagickException("Imagick::getImageOrientation() is not supported in elephc");
    }
    public function getImagePage(): array {
        throw new ImagickException("Imagick::getImagePage() is not supported in elephc");
    }
    public function getImageProfile(string $name): string {
        $_u_name = $name;
        throw new ImagickException("Imagick::getImageProfile() is not supported in elephc");
    }
    public function getImageProfiles(string $pattern = "*", bool $include_values = true): array {
        $_u_pattern = $pattern;
        $_u_include_values = $include_values;
        throw new ImagickException("Imagick::getImageProfiles() is not supported in elephc");
    }
    public function getImageProperties(string $pattern = "*", bool $include_values = true): array {
        $_u_pattern = $pattern;
        $_u_include_values = $include_values;
        throw new ImagickException("Imagick::getImageProperties() is not supported in elephc");
    }
    public function getImageProperty(string $name): string {
        $_u_name = $name;
        throw new ImagickException("Imagick::getImageProperty() is not supported in elephc");
    }
    public function getImageRedPrimary(): array {
        throw new ImagickException("Imagick::getImageRedPrimary() is not supported in elephc");
    }
    public function getImageRegion(int $width, int $height, int $x, int $y): Imagick {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::getImageRegion() is not supported in elephc");
    }
    public function getImageRenderingIntent(): int {
        throw new ImagickException("Imagick::getImageRenderingIntent() is not supported in elephc");
    }
    public function getImageResolution(): array {
        throw new ImagickException("Imagick::getImageResolution() is not supported in elephc");
    }
    public function getImageScene(): int {
        throw new ImagickException("Imagick::getImageScene() is not supported in elephc");
    }
    public function getImageSignature(): string {
        throw new ImagickException("Imagick::getImageSignature() is not supported in elephc");
    }
    public function getImageSize(): int {
        throw new ImagickException("Imagick::getImageSize() is not supported in elephc");
    }
    public function getImageTicksPerSecond(): int {
        throw new ImagickException("Imagick::getImageTicksPerSecond() is not supported in elephc");
    }
    public function getImageTotalInkDensity(): float {
        throw new ImagickException("Imagick::getImageTotalInkDensity() is not supported in elephc");
    }
    public function getImageType(): int {
        throw new ImagickException("Imagick::getImageType() is not supported in elephc");
    }
    public function getImageUnits(): int {
        throw new ImagickException("Imagick::getImageUnits() is not supported in elephc");
    }
    public function getImageVirtualPixelMethod(): int {
        throw new ImagickException("Imagick::getImageVirtualPixelMethod() is not supported in elephc");
    }
    public function getImageWhitePoint(): array {
        throw new ImagickException("Imagick::getImageWhitePoint() is not supported in elephc");
    }
    public function getInterlaceScheme(): int {
        throw new ImagickException("Imagick::getInterlaceScheme() is not supported in elephc");
    }
    public function getOption(string $key): string {
        $_u_key = $key;
        throw new ImagickException("Imagick::getOption() is not supported in elephc");
    }
    public static function getPackageName(): string {
        throw new ImagickException("Imagick::getPackageName() is not supported in elephc");
    }
    public function getPage(): array {
        throw new ImagickException("Imagick::getPage() is not supported in elephc");
    }
    public function getPixelRegionIterator(int $x, int $y, int $columns, int $rows): ImagickPixelIterator {
        $_u_x = $x;
        $_u_y = $y;
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new ImagickException("Imagick::getPixelRegionIterator() is not supported in elephc");
    }
    public function getPointSize(): float {
        throw new ImagickException("Imagick::getPointSize() is not supported in elephc");
    }
    public static function getQuantum(): int {
        throw new ImagickException("Imagick::getQuantum() is not supported in elephc");
    }
    public static function getQuantumDepth(): array {
        throw new ImagickException("Imagick::getQuantumDepth() is not supported in elephc");
    }
    public static function getQuantumRange(): array {
        throw new ImagickException("Imagick::getQuantumRange() is not supported in elephc");
    }
    public static function getRegistry(string $key): string {
        $_u_key = $key;
        throw new ImagickException("Imagick::getRegistry() is not supported in elephc");
    }
    public static function getReleaseDate(): string {
        throw new ImagickException("Imagick::getReleaseDate() is not supported in elephc");
    }
    public static function getResource(int $type): int {
        $_u_type = $type;
        throw new ImagickException("Imagick::getResource() is not supported in elephc");
    }
    public static function getResourceLimit(int $type): int {
        $_u_type = $type;
        throw new ImagickException("Imagick::getResourceLimit() is not supported in elephc");
    }
    public function getSamplingFactors(): array {
        throw new ImagickException("Imagick::getSamplingFactors() is not supported in elephc");
    }
    public function getSize(): array {
        throw new ImagickException("Imagick::getSize() is not supported in elephc");
    }
    public function getSizeOffset(): int {
        throw new ImagickException("Imagick::getSizeOffset() is not supported in elephc");
    }
    public static function getVersion(): array {
        throw new ImagickException("Imagick::getVersion() is not supported in elephc");
    }
    public function haldClutImage(Imagick $clut, int $channel = 0): bool {
        $_u_clut = $clut;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::haldClutImage() is not supported in elephc");
    }
    public function hasNextImage(): bool {
        throw new ImagickException("Imagick::hasNextImage() is not supported in elephc");
    }
    public function hasPreviousImage(): bool {
        throw new ImagickException("Imagick::hasPreviousImage() is not supported in elephc");
    }
    public function identifyFormat(string $embedText): string {
        $_u_embedText = $embedText;
        throw new ImagickException("Imagick::identifyFormat() is not supported in elephc");
    }
    public function identifyImage(bool $appendRawOutput = false): array {
        $_u_appendRawOutput = $appendRawOutput;
        throw new ImagickException("Imagick::identifyImage() is not supported in elephc");
    }
    public function implodeImage(float $radius): bool {
        $_u_radius = $radius;
        throw new ImagickException("Imagick::implodeImage() is not supported in elephc");
    }
    public function importImagePixels(int $x, int $y, int $width, int $height, string $map, int $storage, array $pixels): bool {
        $_u_x = $x;
        $_u_y = $y;
        $_u_width = $width;
        $_u_height = $height;
        $_u_map = $map;
        $_u_storage = $storage;
        $_u_pixels = $pixels;
        throw new ImagickException("Imagick::importImagePixels() is not supported in elephc");
    }
    public function inverseFourierTransformImage(Imagick $complement, bool $magnitude): bool {
        $_u_complement = $complement;
        $_u_magnitude = $magnitude;
        throw new ImagickException("Imagick::inverseFourierTransformImage() is not supported in elephc");
    }
    public function labelImage(string $label): bool {
        $_u_label = $label;
        throw new ImagickException("Imagick::labelImage() is not supported in elephc");
    }
    public function levelImage(float $blackPoint, float $gamma, float $whitePoint, int $channel = 0): bool {
        $_u_blackPoint = $blackPoint;
        $_u_gamma = $gamma;
        $_u_whitePoint = $whitePoint;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::levelImage() is not supported in elephc");
    }
    public function linearStretchImage(float $blackPoint, float $whitePoint): bool {
        $_u_blackPoint = $blackPoint;
        $_u_whitePoint = $whitePoint;
        throw new ImagickException("Imagick::linearStretchImage() is not supported in elephc");
    }
    public static function listRegistry(): array {
        throw new ImagickException("Imagick::listRegistry() is not supported in elephc");
    }
    public function magnifyImage(): bool {
        throw new ImagickException("Imagick::magnifyImage() is not supported in elephc");
    }
    public function mapImage(Imagick $map, bool $dither): bool {
        $_u_map = $map;
        $_u_dither = $dither;
        throw new ImagickException("Imagick::mapImage() is not supported in elephc");
    }
    public function matteFloodfillImage(float $alpha, float $fuzz, $bordercolor, int $x, int $y): bool {
        $_u_alpha = $alpha;
        $_u_fuzz = $fuzz;
        $_u_bordercolor = $bordercolor;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::matteFloodfillImage() is not supported in elephc");
    }
    public function medianFilterImage(float $radius): bool {
        $_u_radius = $radius;
        throw new ImagickException("Imagick::medianFilterImage() is not supported in elephc");
    }
    public function mergeImageLayers(int $layer_method): Imagick {
        $_u_layer_method = $layer_method;
        throw new ImagickException("Imagick::mergeImageLayers() is not supported in elephc");
    }
    public function minifyImage(): bool {
        throw new ImagickException("Imagick::minifyImage() is not supported in elephc");
    }
    public function montageImage(ImagickDraw $draw, string $tile_geometry, string $thumbnail_geometry, int $mode, string $frame): Imagick {
        $_u_draw = $draw;
        $_u_tile_geometry = $tile_geometry;
        $_u_thumbnail_geometry = $thumbnail_geometry;
        $_u_mode = $mode;
        $_u_frame = $frame;
        throw new ImagickException("Imagick::montageImage() is not supported in elephc");
    }
    public function morphImages(int $number_frames): Imagick {
        $_u_number_frames = $number_frames;
        throw new ImagickException("Imagick::morphImages() is not supported in elephc");
    }
    public function morphology(int $morphologyMethod, int $iterations, ImagickKernel $imagickKernel, int $channel = 0): bool {
        $_u_morphologyMethod = $morphologyMethod;
        $_u_iterations = $iterations;
        $_u_imagickKernel = $imagickKernel;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::morphology() is not supported in elephc");
    }
    public function mosaicImages(): Imagick {
        throw new ImagickException("Imagick::mosaicImages() is not supported in elephc");
    }
    public function motionBlurImage(float $radius, float $sigma, float $angle, int $channel = 0): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_angle = $angle;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::motionBlurImage() is not supported in elephc");
    }
    public function newPseudoImage(int $columns, int $rows, string $pseudoString): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        $_u_pseudoString = $pseudoString;
        throw new ImagickException("Imagick::newPseudoImage() is not supported in elephc");
    }
    public function normalizeImage(int $channel = 0): bool {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::normalizeImage() is not supported in elephc");
    }
    public function oilPaintImage(float $radius): bool {
        $_u_radius = $radius;
        throw new ImagickException("Imagick::oilPaintImage() is not supported in elephc");
    }
    public function opaquePaintImage($target, $fill, float $fuzz, bool $invert, int $channel = 0): bool {
        $_u_target = $target;
        $_u_fill = $fill;
        $_u_fuzz = $fuzz;
        $_u_invert = $invert;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::opaquePaintImage() is not supported in elephc");
    }
    public function optimizeImageLayers(): bool {
        throw new ImagickException("Imagick::optimizeImageLayers() is not supported in elephc");
    }
    public function orderedPosterizeImage(string $threshold_map, int $channel = 0): bool {
        $_u_threshold_map = $threshold_map;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::orderedPosterizeImage() is not supported in elephc");
    }
    public function paintFloodfillImage($fill, float $fuzz, $bordercolor, int $x, int $y, int $channel = 0): bool {
        $_u_fill = $fill;
        $_u_fuzz = $fuzz;
        $_u_bordercolor = $bordercolor;
        $_u_x = $x;
        $_u_y = $y;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::paintFloodfillImage() is not supported in elephc");
    }
    public function paintOpaqueImage($target, $fill, float $fuzz, int $channel = 0): bool {
        $_u_target = $target;
        $_u_fill = $fill;
        $_u_fuzz = $fuzz;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::paintOpaqueImage() is not supported in elephc");
    }
    public function paintTransparentImage($target, float $alpha, float $fuzz): bool {
        $_u_target = $target;
        $_u_alpha = $alpha;
        $_u_fuzz = $fuzz;
        throw new ImagickException("Imagick::paintTransparentImage() is not supported in elephc");
    }
    public function pingImage(string $filename): bool {
        $_u_filename = $filename;
        throw new ImagickException("Imagick::pingImage() is not supported in elephc");
    }
    public function pingImageBlob(string $image): bool {
        $_u_image = $image;
        throw new ImagickException("Imagick::pingImageBlob() is not supported in elephc");
    }
    public function pingImageFile($filehandle, string $fileName = ""): bool {
        $_u_filehandle = $filehandle;
        $_u_fileName = $fileName;
        throw new ImagickException("Imagick::pingImageFile() is not supported in elephc");
    }
    public function polaroidImage(ImagickDraw $properties, float $angle): bool {
        $_u_properties = $properties;
        $_u_angle = $angle;
        throw new ImagickException("Imagick::polaroidImage() is not supported in elephc");
    }
    public function posterizeImage(int $levels, bool $dither): bool {
        $_u_levels = $levels;
        $_u_dither = $dither;
        throw new ImagickException("Imagick::posterizeImage() is not supported in elephc");
    }
    public function previewImages(int $preview): bool {
        $_u_preview = $preview;
        throw new ImagickException("Imagick::previewImages() is not supported in elephc");
    }
    public function profileImage(string $name, string $profile = ""): bool {
        $_u_name = $name;
        $_u_profile = $profile;
        throw new ImagickException("Imagick::profileImage() is not supported in elephc");
    }
    public function quantizeImage(int $numberColors, int $colorspace, int $treedepth, bool $dither, bool $measureError): bool {
        $_u_numberColors = $numberColors;
        $_u_colorspace = $colorspace;
        $_u_treedepth = $treedepth;
        $_u_dither = $dither;
        $_u_measureError = $measureError;
        throw new ImagickException("Imagick::quantizeImage() is not supported in elephc");
    }
    public function quantizeImages(int $numberColors, int $colorspace, int $treedepth, bool $dither, bool $measureError): bool {
        $_u_numberColors = $numberColors;
        $_u_colorspace = $colorspace;
        $_u_treedepth = $treedepth;
        $_u_dither = $dither;
        $_u_measureError = $measureError;
        throw new ImagickException("Imagick::quantizeImages() is not supported in elephc");
    }
    public function queryFontMetrics(ImagickDraw $properties, string $text, bool $multiline = false): array {
        $_u_properties = $properties;
        $_u_text = $text;
        $_u_multiline = $multiline;
        throw new ImagickException("Imagick::queryFontMetrics() is not supported in elephc");
    }
    public static function queryFonts(string $pattern = "*"): array {
        $_u_pattern = $pattern;
        throw new ImagickException("Imagick::queryFonts() is not supported in elephc");
    }
    public function radialBlurImage(float $angle, int $channel = 0): bool {
        $_u_angle = $angle;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::radialBlurImage() is not supported in elephc");
    }
    public function raiseImage(int $width, int $height, int $x, int $y, bool $raise): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        $_u_raise = $raise;
        throw new ImagickException("Imagick::raiseImage() is not supported in elephc");
    }
    public function randomThresholdImage(float $low, float $high, int $channel = 0): bool {
        $_u_low = $low;
        $_u_high = $high;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::randomThresholdImage() is not supported in elephc");
    }
    public function readImageFile($filehandle, string $fileName = ""): bool {
        $_u_filehandle = $filehandle;
        $_u_fileName = $fileName;
        throw new ImagickException("Imagick::readImageFile() is not supported in elephc");
    }
    public function readImages(array $filenames): bool {
        $_u_filenames = $filenames;
        throw new ImagickException("Imagick::readImages() is not supported in elephc");
    }
    public function recolorImage(array $matrix): bool {
        $_u_matrix = $matrix;
        throw new ImagickException("Imagick::recolorImage() is not supported in elephc");
    }
    public function reduceNoiseImage(float $radius): bool {
        $_u_radius = $radius;
        throw new ImagickException("Imagick::reduceNoiseImage() is not supported in elephc");
    }
    public function remapImage(Imagick $replacement, int $dITHER): bool {
        $_u_replacement = $replacement;
        $_u_dITHER = $dITHER;
        throw new ImagickException("Imagick::remapImage() is not supported in elephc");
    }
    public function removeImage(): bool {
        throw new ImagickException("Imagick::removeImage() is not supported in elephc");
    }
    public function removeImageProfile(string $name): string {
        $_u_name = $name;
        throw new ImagickException("Imagick::removeImageProfile() is not supported in elephc");
    }
    public function render(): bool {
        throw new ImagickException("Imagick::render() is not supported in elephc");
    }
    public function resampleImage(float $x_resolution, float $y_resolution, int $filter, float $blur): bool {
        $_u_x_resolution = $x_resolution;
        $_u_y_resolution = $y_resolution;
        $_u_filter = $filter;
        $_u_blur = $blur;
        throw new ImagickException("Imagick::resampleImage() is not supported in elephc");
    }
    public function resetImagePage(string $page): bool {
        $_u_page = $page;
        throw new ImagickException("Imagick::resetImagePage() is not supported in elephc");
    }
    public function rollImage(int $x, int $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::rollImage() is not supported in elephc");
    }
    public function rotationalBlurImage(float $angle, int $channel = 0): bool {
        $_u_angle = $angle;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::rotationalBlurImage() is not supported in elephc");
    }
    public function roundCorners(float $x_rounding, float $y_rounding, float $stroke_width = 10, float $displace = 5, float $size_correction = -6): bool {
        $_u_x_rounding = $x_rounding;
        $_u_y_rounding = $y_rounding;
        $_u_stroke_width = $stroke_width;
        $_u_displace = $displace;
        $_u_size_correction = $size_correction;
        throw new ImagickException("Imagick::roundCorners() is not supported in elephc");
    }
    public function sampleImage(int $columns, int $rows): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new ImagickException("Imagick::sampleImage() is not supported in elephc");
    }
    public function segmentImage(int $cOLORSPACE, float $cluster_threshold, float $smooth_threshold, bool $verbose = false): bool {
        $_u_cOLORSPACE = $cOLORSPACE;
        $_u_cluster_threshold = $cluster_threshold;
        $_u_smooth_threshold = $smooth_threshold;
        $_u_verbose = $verbose;
        throw new ImagickException("Imagick::segmentImage() is not supported in elephc");
    }
    public function selectiveBlurImage(float $radius, float $sigma, float $threshold, int $channel = 0): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_threshold = $threshold;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::selectiveBlurImage() is not supported in elephc");
    }
    public function separateImageChannel(int $channel): bool {
        $_u_channel = $channel;
        throw new ImagickException("Imagick::separateImageChannel() is not supported in elephc");
    }
    public function sepiaToneImage(float $threshold): bool {
        $_u_threshold = $threshold;
        throw new ImagickException("Imagick::sepiaToneImage() is not supported in elephc");
    }
    public function setBackgroundColor($background): bool {
        $_u_background = $background;
        throw new ImagickException("Imagick::setBackgroundColor() is not supported in elephc");
    }
    public function setColorspace(int $cOLORSPACE): bool {
        $_u_cOLORSPACE = $cOLORSPACE;
        throw new ImagickException("Imagick::setColorspace() is not supported in elephc");
    }
    public function setCompression(int $compression): bool {
        $_u_compression = $compression;
        throw new ImagickException("Imagick::setCompression() is not supported in elephc");
    }
    public function setFilename(string $filename): bool {
        $_u_filename = $filename;
        throw new ImagickException("Imagick::setFilename() is not supported in elephc");
    }
    public function setFont(string $font): bool {
        $_u_font = $font;
        throw new ImagickException("Imagick::setFont() is not supported in elephc");
    }
    public function setGravity(int $gravity): bool {
        $_u_gravity = $gravity;
        throw new ImagickException("Imagick::setGravity() is not supported in elephc");
    }
    public function setImage(Imagick $replace): bool {
        $_u_replace = $replace;
        throw new ImagickException("Imagick::setImage() is not supported in elephc");
    }
    public function setImageAlphaChannel(int $mode): bool {
        $_u_mode = $mode;
        throw new ImagickException("Imagick::setImageAlphaChannel() is not supported in elephc");
    }
    public function setImageArtifact(string $artifact, string $value): bool {
        $_u_artifact = $artifact;
        $_u_value = $value;
        throw new ImagickException("Imagick::setImageArtifact() is not supported in elephc");
    }
    public function setImageAttribute(string $key, string $value): bool {
        $_u_key = $key;
        $_u_value = $value;
        throw new ImagickException("Imagick::setImageAttribute() is not supported in elephc");
    }
    public function setImageBias(float $bias): bool {
        $_u_bias = $bias;
        throw new ImagickException("Imagick::setImageBias() is not supported in elephc");
    }
    public function setImageBiasQuantum(float $bias): void {
        $_u_bias = $bias;
        throw new ImagickException("Imagick::setImageBiasQuantum() is not supported in elephc");
    }
    public function setImageBluePrimary(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::setImageBluePrimary() is not supported in elephc");
    }
    public function setImageBorderColor($border): bool {
        $_u_border = $border;
        throw new ImagickException("Imagick::setImageBorderColor() is not supported in elephc");
    }
    public function setImageChannelDepth(int $channel, int $depth): bool {
        $_u_channel = $channel;
        $_u_depth = $depth;
        throw new ImagickException("Imagick::setImageChannelDepth() is not supported in elephc");
    }
    public function setImageClipMask(Imagick $clip_mask): bool {
        $_u_clip_mask = $clip_mask;
        throw new ImagickException("Imagick::setImageClipMask() is not supported in elephc");
    }
    public function setImageColormapColor(int $index, ImagickPixel $color): bool {
        $_u_index = $index;
        $_u_color = $color;
        throw new ImagickException("Imagick::setImageColormapColor() is not supported in elephc");
    }
    public function setImageColorspace(int $colorspace): bool {
        $_u_colorspace = $colorspace;
        throw new ImagickException("Imagick::setImageColorspace() is not supported in elephc");
    }
    public function setImageCompose(int $compose): bool {
        $_u_compose = $compose;
        throw new ImagickException("Imagick::setImageCompose() is not supported in elephc");
    }
    public function setImageCompression(int $compression): bool {
        $_u_compression = $compression;
        throw new ImagickException("Imagick::setImageCompression() is not supported in elephc");
    }
    public function setImageDelay(int $delay): bool {
        $_u_delay = $delay;
        throw new ImagickException("Imagick::setImageDelay() is not supported in elephc");
    }
    public function setImageDepth(int $depth): bool {
        $_u_depth = $depth;
        throw new ImagickException("Imagick::setImageDepth() is not supported in elephc");
    }
    public function setImageDispose(int $dispose): bool {
        $_u_dispose = $dispose;
        throw new ImagickException("Imagick::setImageDispose() is not supported in elephc");
    }
    public function setImageExtent(int $columns, int $rows): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new ImagickException("Imagick::setImageExtent() is not supported in elephc");
    }
    public function setImageFilename(string $filename): bool {
        $_u_filename = $filename;
        throw new ImagickException("Imagick::setImageFilename() is not supported in elephc");
    }
    public function setImageGamma(float $gamma): bool {
        $_u_gamma = $gamma;
        throw new ImagickException("Imagick::setImageGamma() is not supported in elephc");
    }
    public function setImageGravity(int $gravity): bool {
        $_u_gravity = $gravity;
        throw new ImagickException("Imagick::setImageGravity() is not supported in elephc");
    }
    public function setImageGreenPrimary(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::setImageGreenPrimary() is not supported in elephc");
    }
    public function setImageInterlaceScheme(int $interlace_scheme): bool {
        $_u_interlace_scheme = $interlace_scheme;
        throw new ImagickException("Imagick::setImageInterlaceScheme() is not supported in elephc");
    }
    public function setImageInterpolateMethod(int $method): bool {
        $_u_method = $method;
        throw new ImagickException("Imagick::setImageInterpolateMethod() is not supported in elephc");
    }
    public function setImageIterations(int $iterations): bool {
        $_u_iterations = $iterations;
        throw new ImagickException("Imagick::setImageIterations() is not supported in elephc");
    }
    public function setImageMatte(bool $matte): bool {
        $_u_matte = $matte;
        throw new ImagickException("Imagick::setImageMatte() is not supported in elephc");
    }
    public function setImageMatteColor($matte): bool {
        $_u_matte = $matte;
        throw new ImagickException("Imagick::setImageMatteColor() is not supported in elephc");
    }
    public function setImageOpacity(float $opacity): bool {
        $_u_opacity = $opacity;
        throw new ImagickException("Imagick::setImageOpacity() is not supported in elephc");
    }
    public function setImageOrientation(int $orientation): bool {
        $_u_orientation = $orientation;
        throw new ImagickException("Imagick::setImageOrientation() is not supported in elephc");
    }
    public function setImagePage(int $width, int $height, int $x, int $y): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::setImagePage() is not supported in elephc");
    }
    public function setImageProfile(string $name, string $profile): bool {
        $_u_name = $name;
        $_u_profile = $profile;
        throw new ImagickException("Imagick::setImageProfile() is not supported in elephc");
    }
    public function setImageProperty(string $name, string $value): bool {
        $_u_name = $name;
        $_u_value = $value;
        throw new ImagickException("Imagick::setImageProperty() is not supported in elephc");
    }
    public function setImageRedPrimary(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::setImageRedPrimary() is not supported in elephc");
    }
    public function setImageRenderingIntent(int $rendering_intent): bool {
        $_u_rendering_intent = $rendering_intent;
        throw new ImagickException("Imagick::setImageRenderingIntent() is not supported in elephc");
    }
    public function setImageResolution(float $x_resolution, float $y_resolution): bool {
        $_u_x_resolution = $x_resolution;
        $_u_y_resolution = $y_resolution;
        throw new ImagickException("Imagick::setImageResolution() is not supported in elephc");
    }
    public function setImageScene(int $scene): bool {
        $_u_scene = $scene;
        throw new ImagickException("Imagick::setImageScene() is not supported in elephc");
    }
    public function setImageTicksPerSecond(int $ticks_per_second): bool {
        $_u_ticks_per_second = $ticks_per_second;
        throw new ImagickException("Imagick::setImageTicksPerSecond() is not supported in elephc");
    }
    public function setImageType(int $image_type): bool {
        $_u_image_type = $image_type;
        throw new ImagickException("Imagick::setImageType() is not supported in elephc");
    }
    public function setImageUnits(int $units): bool {
        $_u_units = $units;
        throw new ImagickException("Imagick::setImageUnits() is not supported in elephc");
    }
    public function setImageVirtualPixelMethod(int $method): bool {
        $_u_method = $method;
        throw new ImagickException("Imagick::setImageVirtualPixelMethod() is not supported in elephc");
    }
    public function setImageWhitePoint(float $x, float $y): bool {
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::setImageWhitePoint() is not supported in elephc");
    }
    public function setInterlaceScheme(int $interlace_scheme): bool {
        $_u_interlace_scheme = $interlace_scheme;
        throw new ImagickException("Imagick::setInterlaceScheme() is not supported in elephc");
    }
    public function setOption(string $key, string $value): bool {
        $_u_key = $key;
        $_u_value = $value;
        throw new ImagickException("Imagick::setOption() is not supported in elephc");
    }
    public function setPage(int $width, int $height, int $x, int $y): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::setPage() is not supported in elephc");
    }
    public function setPointSize(float $point_size): bool {
        $_u_point_size = $point_size;
        throw new ImagickException("Imagick::setPointSize() is not supported in elephc");
    }
    public function setProgressMonitor($callback): bool {
        $_u_callback = $callback;
        throw new ImagickException("Imagick::setProgressMonitor() is not supported in elephc");
    }
    public static function setRegistry(string $key, string $value): bool {
        $_u_key = $key;
        $_u_value = $value;
        throw new ImagickException("Imagick::setRegistry() is not supported in elephc");
    }
    public function setResolution(float $x_resolution, float $y_resolution): bool {
        $_u_x_resolution = $x_resolution;
        $_u_y_resolution = $y_resolution;
        throw new ImagickException("Imagick::setResolution() is not supported in elephc");
    }
    public static function setResourceLimit(int $type, int $limit): bool {
        $_u_type = $type;
        $_u_limit = $limit;
        throw new ImagickException("Imagick::setResourceLimit() is not supported in elephc");
    }
    public function setSamplingFactors(array $factors): bool {
        $_u_factors = $factors;
        throw new ImagickException("Imagick::setSamplingFactors() is not supported in elephc");
    }
    public function setSize(int $columns, int $rows): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new ImagickException("Imagick::setSize() is not supported in elephc");
    }
    public function setSizeOffset(int $columns, int $rows, int $offset): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        $_u_offset = $offset;
        throw new ImagickException("Imagick::setSizeOffset() is not supported in elephc");
    }
    public function setType(int $image_type): bool {
        $_u_image_type = $image_type;
        throw new ImagickException("Imagick::setType() is not supported in elephc");
    }
    public function shadeImage(bool $gray, float $azimuth, float $elevation): bool {
        $_u_gray = $gray;
        $_u_azimuth = $azimuth;
        $_u_elevation = $elevation;
        throw new ImagickException("Imagick::shadeImage() is not supported in elephc");
    }
    public function shadowImage(float $opacity, float $sigma, int $x, int $y): bool {
        $_u_opacity = $opacity;
        $_u_sigma = $sigma;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::shadowImage() is not supported in elephc");
    }
    public function shaveImage(int $columns, int $rows): bool {
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new ImagickException("Imagick::shaveImage() is not supported in elephc");
    }
    public function shearImage($background, float $x_shear, float $y_shear): bool {
        $_u_background = $background;
        $_u_x_shear = $x_shear;
        $_u_y_shear = $y_shear;
        throw new ImagickException("Imagick::shearImage() is not supported in elephc");
    }
    public function sigmoidalContrastImage(bool $sharpen, float $alpha, float $beta, int $channel = 0): bool {
        $_u_sharpen = $sharpen;
        $_u_alpha = $alpha;
        $_u_beta = $beta;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::sigmoidalContrastImage() is not supported in elephc");
    }
    public function sketchImage(float $radius, float $sigma, float $angle): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_angle = $angle;
        throw new ImagickException("Imagick::sketchImage() is not supported in elephc");
    }
    public function smushImages(bool $stack, int $offset): Imagick {
        $_u_stack = $stack;
        $_u_offset = $offset;
        throw new ImagickException("Imagick::smushImages() is not supported in elephc");
    }
    public function solarizeImage(int $threshold): bool {
        $_u_threshold = $threshold;
        throw new ImagickException("Imagick::solarizeImage() is not supported in elephc");
    }
    public function sparseColorImage(int $sPARSE_METHOD, array $arguments, int $channel = 0): bool {
        $_u_sPARSE_METHOD = $sPARSE_METHOD;
        $_u_arguments = $arguments;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::sparseColorImage() is not supported in elephc");
    }
    public function spliceImage(int $width, int $height, int $x, int $y): bool {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::spliceImage() is not supported in elephc");
    }
    public function spreadImage(float $radius): bool {
        $_u_radius = $radius;
        throw new ImagickException("Imagick::spreadImage() is not supported in elephc");
    }
    public function statisticImage(int $type, int $width, int $height, int $channel = 0): bool {
        $_u_type = $type;
        $_u_width = $width;
        $_u_height = $height;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::statisticImage() is not supported in elephc");
    }
    public function steganoImage(Imagick $watermark_wand, int $offset): Imagick {
        $_u_watermark_wand = $watermark_wand;
        $_u_offset = $offset;
        throw new ImagickException("Imagick::steganoImage() is not supported in elephc");
    }
    public function stereoImage(Imagick $offset_wand): bool {
        $_u_offset_wand = $offset_wand;
        throw new ImagickException("Imagick::stereoImage() is not supported in elephc");
    }
    public function stripImage(): bool {
        throw new ImagickException("Imagick::stripImage() is not supported in elephc");
    }
    public function subImageMatch(Imagick $imagick, array &$offset, float &$similarity): Imagick {
        $_u_imagick = $imagick;
        $_u_offset = $offset;
        $_u_similarity = $similarity;
        throw new ImagickException("Imagick::subImageMatch() is not supported in elephc");
    }
    public function thresholdImage(float $threshold, int $channel = 0): bool {
        $_u_threshold = $threshold;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::thresholdImage() is not supported in elephc");
    }
    public function tintImage($tint, $opacity, bool $legacy = false): bool {
        $_u_tint = $tint;
        $_u_opacity = $opacity;
        $_u_legacy = $legacy;
        throw new ImagickException("Imagick::tintImage() is not supported in elephc");
    }
    public function __toString(): string {
        throw new ImagickException("Imagick::__toString() is not supported in elephc");
    }
    public function transformImage(string $crop, string $geometry): Imagick {
        $_u_crop = $crop;
        $_u_geometry = $geometry;
        throw new ImagickException("Imagick::transformImage() is not supported in elephc");
    }
    public function transformImageColorspace(int $colorspace): bool {
        $_u_colorspace = $colorspace;
        throw new ImagickException("Imagick::transformImageColorspace() is not supported in elephc");
    }
    public function transparentPaintImage($target, float $alpha, float $fuzz, bool $invert): bool {
        $_u_target = $target;
        $_u_alpha = $alpha;
        $_u_fuzz = $fuzz;
        $_u_invert = $invert;
        throw new ImagickException("Imagick::transparentPaintImage() is not supported in elephc");
    }
    public function transposeImage(): bool {
        throw new ImagickException("Imagick::transposeImage() is not supported in elephc");
    }
    public function transverseImage(): bool {
        throw new ImagickException("Imagick::transverseImage() is not supported in elephc");
    }
    public function trimImage(float $fuzz): bool {
        $_u_fuzz = $fuzz;
        throw new ImagickException("Imagick::trimImage() is not supported in elephc");
    }
    public function uniqueImageColors(): bool {
        throw new ImagickException("Imagick::uniqueImageColors() is not supported in elephc");
    }
    public function unsharpMaskImage(float $radius, float $sigma, float $amount, float $threshold, int $channel = 0): bool {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_amount = $amount;
        $_u_threshold = $threshold;
        $_u_channel = $channel;
        throw new ImagickException("Imagick::unsharpMaskImage() is not supported in elephc");
    }
    public function vignetteImage(float $blackPoint, float $whitePoint, int $x, int $y): bool {
        $_u_blackPoint = $blackPoint;
        $_u_whitePoint = $whitePoint;
        $_u_x = $x;
        $_u_y = $y;
        throw new ImagickException("Imagick::vignetteImage() is not supported in elephc");
    }
    public function whiteThresholdImage($threshold): bool {
        $_u_threshold = $threshold;
        throw new ImagickException("Imagick::whiteThresholdImage() is not supported in elephc");
    }
    public function writeImageFile($filehandle, string $format = ""): bool {
        $_u_filehandle = $filehandle;
        $_u_format = $format;
        throw new ImagickException("Imagick::writeImageFile() is not supported in elephc");
    }
    public function writeImagesFile($filehandle, string $format = ""): bool {
        $_u_filehandle = $filehandle;
        $_u_format = $format;
        throw new ImagickException("Imagick::writeImagesFile() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---

}

class ImagickPixelIterator implements Iterator {
    private int $wand = 0;
    private int $row = 0;
    private int $width = 0;
    private int $height = 0;

    public function __construct(Imagick $wand) {
        $this->wand = $wand->_wandHandle();
        $_w = elephc_imagick_cur_width($this->wand);
        $_h = elephc_imagick_cur_height($this->wand);
        $this->width = $_w < 0 ? 0 : $_w;
        $this->height = $_h < 0 ? 0 : $_h;
        $this->row = 0;
    }

    // Returns the current row as an array of ImagickPixel objects.
    public function getCurrentIteratorRow(): array {
        $_pixels = [];
        for ($_x = 0; $_x < $this->width; $_x++) {
            $_packed = elephc_imagick_pixel_color($this->wand, $_x, $this->row);
            $_pixels[] = _imagick_pixel_from_int($_packed);
        }
        return $_pixels;
    }

    public function getNextIteratorRow(): array {
        $this->row = $this->row + 1;
        return $this->getCurrentIteratorRow();
    }

    public function getIteratorIndex(): int {
        return $this->row;
    }

    public function setIteratorRow(int $row): bool {
        if ($row < 0 || $row >= $this->height) {
            return false;
        }
        $this->row = $row;
        return true;
    }

    public function rewind(): void {
        $this->row = 0;
    }

    public function valid(): bool {
        return $this->row < $this->height;
    }

    public function current(): mixed {
        return $this->getCurrentIteratorRow();
    }

    public function key(): mixed {
        return $this->row;
    }

    public function next(): void {
        $this->row = $this->row + 1;
    }

    public function clear(): bool {
        return true;
    }

    public function destroy(): bool {
        return true;
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function getIteratorRow(): int {
        throw new ImagickPixelIteratorException("ImagickPixelIterator::getIteratorRow() is not supported in elephc");
    }
    public function getPreviousIteratorRow(): array {
        throw new ImagickPixelIteratorException("ImagickPixelIterator::getPreviousIteratorRow() is not supported in elephc");
    }
    public function newPixelIterator(Imagick $wand): bool {
        $_u_wand = $wand;
        throw new ImagickPixelIteratorException("ImagickPixelIterator::newPixelIterator() is not supported in elephc");
    }
    public function newPixelRegionIterator(Imagick $wand, int $x, int $y, int $columns, int $rows): bool {
        $_u_wand = $wand;
        $_u_x = $x;
        $_u_y = $y;
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new ImagickPixelIteratorException("ImagickPixelIterator::newPixelRegionIterator() is not supported in elephc");
    }
    public function resetIterator(): bool {
        throw new ImagickPixelIteratorException("ImagickPixelIterator::resetIterator() is not supported in elephc");
    }
    public function setIteratorFirstRow(): bool {
        throw new ImagickPixelIteratorException("ImagickPixelIterator::setIteratorFirstRow() is not supported in elephc");
    }
    public function setIteratorLastRow(): bool {
        throw new ImagickPixelIteratorException("ImagickPixelIterator::setIteratorLastRow() is not supported in elephc");
    }
    public function syncIterator(): bool {
        throw new ImagickPixelIteratorException("ImagickPixelIterator::syncIterator() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---

}

// ===========================================================================
// Gmagick OOP. GraphicsMagick-style API over the SAME wand bridge as
// Imagick (elephc_imagick_* / elephc_idraw_*) and the shared _imagick_* color
// helpers. Differences from Imagick: Gmagick does NOT implement Iterator or
// Countable, most mutating methods return the Gmagick object (fluent), and there
// is no pixel iterator or kernel type. Unsupported effects throw GmagickException.
// ---------------------------------------------------------------------------

class GmagickException extends Exception {
}

class GmagickDrawException extends Exception {
}

class GmagickPixelException extends Exception {
}

// Parses a color string into a GD packed color, translating the shared parser's
// ImagickPixelException into a GmagickPixelException so Gmagick callers catch the
// exception type their API documents.
function _gmagick_parse_color(string $c): int {
    try {
        return _imagick_parse_color($c);
    } catch (ImagickPixelException $e) {
        throw new GmagickPixelException($e->getMessage());
    }
}

// Normalizes a color argument (string or GmagickPixel) into a GD packed color.
function _gmagick_norm_color($color): int {
    if (is_string($color)) {
        return _gmagick_parse_color($color);
    }
    // instanceof narrows the Mixed argument so the property read is allowed; the
    // (int) cast resolves the Mixed property type back to int.
    if ($color instanceof GmagickPixel) {
        return (int) $color->packed;
    }
    return 0;
}

// Wraps a GD packed color into a fresh GmagickPixel.
function _gmagick_pixel_from_int(int $packed): GmagickPixel {
    $_p = new GmagickPixel("black");
    $_p->packed = $packed;
    return $_p;
}

class GmagickPixel {
    public int $packed = 0;

    public function __construct(string $color = "black") {
        $this->packed = _gmagick_parse_color($color);
    }

    public function setColor(string $color): GmagickPixel {
        $this->packed = _gmagick_parse_color($color);
        return $this;
    }

    // Returns the color as an associative array of channels. With $normalized != 0
    // the channels are 0..1 floats; otherwise 0..255 integers. No return type hint
    // so the inferred associative type lets callers read the "r"/"g"/"b"/"a" keys.
    // (Gmagick's getColor toggles a textual form via a flag; elephc always returns
    // the channel array and exposes the string form through getColorAsString(), to
    // avoid an unresolvable string|array union return — documented difference.)
    public function getColor(int $normalized = 0) {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        $_gd = ($this->packed >> 24) & 0x7F;
        $_a = 255 - (int) ($_gd * 255 / 127);
        if ($normalized !== 0) {
            return ["r" => $_r / 255, "g" => $_g / 255, "b" => $_b / 255, "a" => $_a / 255];
        }
        return ["r" => $_r, "g" => $_g, "b" => $_b, "a" => $_a];
    }

    // Returns the color as an "srgb(r,g,b)" string (opaque form).
    public function getColorAsString(): string {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        return "srgb(" . $_r . "," . $_g . "," . $_b . ")";
    }

    // Returns one channel as a 0..1 float, selected by a Gmagick::COLOR_* code.
    public function getColorValue(int $color): float {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        $_gd = ($this->packed >> 24) & 0x7F;
        $_a = 255 - (int) ($_gd * 255 / 127);
        if ($color === 4) { return $_r / 255; }
        if ($color === 3) { return $_g / 255; }
        if ($color === 1) { return $_b / 255; }
        if ($color === 8) { return $_a / 255; }
        if ($color === 7) { return (255 - $_a) / 255; }
        return 0.0;
    }

    // Sets one channel from a 0..1 float, selected by a Gmagick::COLOR_* code.
    public function setColorValue(int $color, float $value): GmagickPixel {
        $_r = ($this->packed >> 16) & 0xFF;
        $_g = ($this->packed >> 8) & 0xFF;
        $_b = $this->packed & 0xFF;
        $_v = (int) round($value * 255);
        if ($_v < 0) { $_v = 0; }
        if ($_v > 255) { $_v = 255; }
        if ($color === 4) { $_r = $_v; }
        if ($color === 3) { $_g = $_v; }
        if ($color === 1) { $_b = $_v; }
        $this->packed = ($_r << 16) | ($_g << 8) | $_b;
        return $this;
    }

    public function clear(): bool {
        return true;
    }

    public function destroy(): bool {
        return true;
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function getcolorcount(): int {
        throw new GmagickPixelException("GmagickPixel::getcolorcount() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---
}

class GmagickDraw {
    private int $draw = 0;

    public function __construct() {
        $this->draw = elephc_idraw_new();
    }

    // Internal: exposes the bridge draw handle to Gmagick::drawImage.
    public function _gmagickHandle(): int {
        return $this->draw;
    }

    public function setFillColor($color): GmagickDraw {
        elephc_idraw_set_fill($this->draw, _gmagick_norm_color($color));
        return $this;
    }

    public function setStrokeColor($color): GmagickDraw {
        elephc_idraw_set_stroke($this->draw, _gmagick_norm_color($color));
        return $this;
    }

    public function setStrokeWidth(float $width): GmagickDraw {
        elephc_idraw_set_stroke_width($this->draw, (int) round($width));
        return $this;
    }

    public function line(float $sx, float $sy, float $ex, float $ey): GmagickDraw {
        elephc_idraw_line($this->draw, (int) round($sx), (int) round($sy), (int) round($ex), (int) round($ey));
        return $this;
    }

    public function rectangle(float $x1, float $y1, float $x2, float $y2): GmagickDraw {
        elephc_idraw_rectangle($this->draw, (int) round($x1), (int) round($y1), (int) round($x2), (int) round($y2));
        return $this;
    }

    public function ellipse(float $ox, float $oy, float $rx, float $ry, float $start, float $end): GmagickDraw {
        $_oxy = _imagick_pack2((int) round($ox), (int) round($oy));
        $_rxy = _imagick_pack2((int) round($rx), (int) round($ry));
        $_se = _imagick_pack2((int) round($start), (int) round($end));
        elephc_idraw_ellipse($this->draw, $_oxy, $_rxy, $_se);
        return $this;
    }

    public function point(float $x, float $y): GmagickDraw {
        elephc_idraw_point($this->draw, (int) round($x), (int) round($y));
        return $this;
    }

    // Draws a filled/stroked polygon. Each coordinate is ["x" => , "y" => ].
    public function polygon(array $coordinates): GmagickDraw {
        elephc_idraw_poly_reset($this->draw);
        $_n = count($coordinates);
        for ($_i = 0; $_i < $_n; $_i++) {
            $_px = (int) round($coordinates[$_i]["x"]);
            $_py = (int) round($coordinates[$_i]["y"]);
            elephc_idraw_poly_point($this->draw, $_px, $_py);
        }
        elephc_idraw_polygon($this->draw);
        return $this;
    }

    // GmagickDraw::annotate requires FreeType text rendering, unsupported here.
    public function annotate(float $x, float $y, string $text): GmagickDraw {
        $_u_x = $x;
        $_u_y = $y;
        $_u_t = $text;
        throw new GmagickDrawException("GmagickDraw::annotate() requires FreeType text, which is not supported in elephc");
    }

    public function clear(): GmagickDraw {
        elephc_idraw_clear($this->draw);
        return $this;
    }

    public function destroy(): bool {
        elephc_idraw_destroy($this->draw);
        $this->draw = 0;
        return true;
    }

    public function __destruct() {
        elephc_idraw_destroy($this->draw);
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function arc(float $sx, float $sy, float $ex, float $ey, float $sd, float $ed): GmagickDraw {
        $_u_sx = $sx;
        $_u_sy = $sy;
        $_u_ex = $ex;
        $_u_ey = $ey;
        $_u_sd = $sd;
        $_u_ed = $ed;
        throw new GmagickDrawException("GmagickDraw::arc() is not supported in elephc");
    }
    public function bezier(array $coordinate_array): GmagickDraw {
        $_u_coordinate_array = $coordinate_array;
        throw new GmagickDrawException("GmagickDraw::bezier() is not supported in elephc");
    }
    public function getfillcolor(): GmagickPixel {
        throw new GmagickDrawException("GmagickDraw::getfillcolor() is not supported in elephc");
    }
    public function getfillopacity(): float {
        throw new GmagickDrawException("GmagickDraw::getfillopacity() is not supported in elephc");
    }
    public function getfont(): mixed {
        throw new GmagickDrawException("GmagickDraw::getfont() is not supported in elephc");
    }
    public function getfontsize(): float {
        throw new GmagickDrawException("GmagickDraw::getfontsize() is not supported in elephc");
    }
    public function getfontstyle(): int {
        throw new GmagickDrawException("GmagickDraw::getfontstyle() is not supported in elephc");
    }
    public function getfontweight(): int {
        throw new GmagickDrawException("GmagickDraw::getfontweight() is not supported in elephc");
    }
    public function getstrokecolor(): GmagickPixel {
        throw new GmagickDrawException("GmagickDraw::getstrokecolor() is not supported in elephc");
    }
    public function getstrokeopacity(): float {
        throw new GmagickDrawException("GmagickDraw::getstrokeopacity() is not supported in elephc");
    }
    public function getstrokewidth(): float {
        throw new GmagickDrawException("GmagickDraw::getstrokewidth() is not supported in elephc");
    }
    public function gettextdecoration(): int {
        throw new GmagickDrawException("GmagickDraw::gettextdecoration() is not supported in elephc");
    }
    public function gettextencoding(): mixed {
        throw new GmagickDrawException("GmagickDraw::gettextencoding() is not supported in elephc");
    }
    public function polyline(array $coordinate_array): GmagickDraw {
        $_u_coordinate_array = $coordinate_array;
        throw new GmagickDrawException("GmagickDraw::polyline() is not supported in elephc");
    }
    public function rotate(float $degrees): GmagickDraw {
        $_u_degrees = $degrees;
        throw new GmagickDrawException("GmagickDraw::rotate() is not supported in elephc");
    }
    public function roundrectangle(float $x1, float $y1, float $x2, float $y2, float $rx, float $ry): GmagickDraw {
        $_u_x1 = $x1;
        $_u_y1 = $y1;
        $_u_x2 = $x2;
        $_u_y2 = $y2;
        $_u_rx = $rx;
        $_u_ry = $ry;
        throw new GmagickDrawException("GmagickDraw::roundrectangle() is not supported in elephc");
    }
    public function scale(float $x, float $y): GmagickDraw {
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickDrawException("GmagickDraw::scale() is not supported in elephc");
    }
    public function setfillopacity(float $fill_opacity): GmagickDraw {
        $_u_fill_opacity = $fill_opacity;
        throw new GmagickDrawException("GmagickDraw::setfillopacity() is not supported in elephc");
    }
    public function setfont(string $font): GmagickDraw {
        $_u_font = $font;
        throw new GmagickDrawException("GmagickDraw::setfont() is not supported in elephc");
    }
    public function setfontsize(float $pointsize): GmagickDraw {
        $_u_pointsize = $pointsize;
        throw new GmagickDrawException("GmagickDraw::setfontsize() is not supported in elephc");
    }
    public function setfontstyle(int $style): GmagickDraw {
        $_u_style = $style;
        throw new GmagickDrawException("GmagickDraw::setfontstyle() is not supported in elephc");
    }
    public function setfontweight(int $weight): GmagickDraw {
        $_u_weight = $weight;
        throw new GmagickDrawException("GmagickDraw::setfontweight() is not supported in elephc");
    }
    public function setstrokeopacity(float $stroke_opacity): GmagickDraw {
        $_u_stroke_opacity = $stroke_opacity;
        throw new GmagickDrawException("GmagickDraw::setstrokeopacity() is not supported in elephc");
    }
    public function settextdecoration(int $decoration): GmagickDraw {
        $_u_decoration = $decoration;
        throw new GmagickDrawException("GmagickDraw::settextdecoration() is not supported in elephc");
    }
    public function settextencoding(string $encoding): GmagickDraw {
        $_u_encoding = $encoding;
        throw new GmagickDrawException("GmagickDraw::settextencoding() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---
}

class Gmagick {
    // Resize filters (accepted for parity; elephc resizes bilinear).
    const FILTER_UNDEFINED = 0;
    const FILTER_POINT = 1;
    const FILTER_BOX = 2;
    const FILTER_TRIANGLE = 3;
    const FILTER_HERMITE = 4;
    const FILTER_HANNING = 5;
    const FILTER_HAMMING = 6;
    const FILTER_BLACKMAN = 7;
    const FILTER_GAUSSIAN = 8;
    const FILTER_QUADRATIC = 9;
    const FILTER_CUBIC = 10;
    const FILTER_CATROM = 11;
    const FILTER_MITCHELL = 12;
    const FILTER_LANCZOS = 22;
    const FILTER_SINC = 19;
    // Composite operators. Only OVER/COPY are implemented; others throw.
    const COMPOSITE_DEFAULT = 40;
    const COMPOSITE_OVER = 40;
    const COMPOSITE_COPY = 42;
    const COMPOSITE_MULTIPLY = 30;
    // Channels.
    const CHANNEL_RED = 1;
    const CHANNEL_GREEN = 2;
    const CHANNEL_BLUE = 4;
    const CHANNEL_ALPHA = 8;
    const CHANNEL_OPACITY = 8;
    const CHANNEL_ALL = 134217727;
    // GmagickPixel color-channel selectors (Gmagick::COLOR_*).
    const COLOR_BLACK = 0;
    const COLOR_BLUE = 1;
    const COLOR_GREEN = 3;
    const COLOR_RED = 4;
    const COLOR_OPACITY = 7;
    const COLOR_ALPHA = 8;
    // Image types (subset).
    const IMGTYPE_UNDEFINED = 0;
    const IMGTYPE_GRAYSCALE = 2;
    const IMGTYPE_PALETTE = 3;
    const IMGTYPE_TRUECOLOR = 6;

    private int $wand = 0;

    public function __construct(?string $filename = null) {
        $this->wand = elephc_imagick_new();
        if ($filename !== null && $filename !== "") {
            $this->readImage((string) $filename);
        }
    }

    // Internal: exposes the bridge wand handle to sibling Gmagick objects.
    public function _wandHandle(): int {
        return $this->wand;
    }

    public function readImage(string $filename): Gmagick {
        if (elephc_imagick_read_file($this->wand, $filename) !== 0) {
            throw new GmagickException("Gmagick::readimage(): unable to read '" . $filename . "'");
        }
        return $this;
    }

    public function readImageBlob(string $image, string $filename = ""): Gmagick {
        $_u_name = $filename;
        $_len = strlen($image);
        if ($_len <= 0) {
            throw new GmagickException("Gmagick::readimageblob(): empty blob");
        }
        $_buf = elephc_img_stage_ptr($_len);
        if (ptr_is_null($_buf)) {
            throw new GmagickException("Gmagick::readimageblob(): allocation failed");
        }
        ptr_write_string($_buf, $image);
        if (elephc_imagick_read_blob($this->wand, $_len) !== 0) {
            throw new GmagickException("Gmagick::readimageblob(): unrecognized image data");
        }
        return $this;
    }

    public function newImage(int $width, int $height, $background, string $format = ""): Gmagick {
        $_bg = _gmagick_norm_color($background);
        $_fmt = $format === "" ? 0 : _imagick_fmt_to_code($format);
        if (elephc_imagick_new_image($this->wand, $width, $height, $_bg, $_fmt) !== 0) {
            throw new GmagickException("Gmagick::newimage(): invalid dimensions");
        }
        return $this;
    }

    public function addImage(Gmagick $source): Gmagick {
        if (elephc_imagick_add_image($this->wand, $source->_wandHandle()) !== 0) {
            throw new GmagickException("Gmagick::addimage(): no source image");
        }
        return $this;
    }

    public function writeImage(string $filename): Gmagick {
        $_fmt = _imagick_fmt_from_path($filename);
        if (elephc_imagick_write_file($this->wand, $filename, $_fmt) !== 0) {
            throw new GmagickException("Gmagick::writeimage(): unable to write '" . $filename . "'");
        }
        return $this;
    }

    public function getImageBlob(): string {
        $_len = elephc_imagick_get_blob($this->wand, 0);
        if ($_len < 0) {
            throw new GmagickException("Gmagick::getimageblob(): no image or encode failed");
        }
        $_bytes = ptr_read_string(elephc_img_encoded_ptr(), $_len);
        elephc_img_encoded_clear();
        return $_bytes;
    }

    public function setImageFormat(string $format): Gmagick {
        $_code = _imagick_fmt_to_code($format);
        if ($_code === 0) {
            throw new GmagickException("Gmagick::setimageformat(): unsupported format '" . $format . "'");
        }
        elephc_imagick_set_format($this->wand, $_code);
        return $this;
    }

    public function getImageFormat(): string {
        return _imagick_code_to_fmt(elephc_imagick_get_format($this->wand));
    }

    public function setCompressionQuality(int $quality): Gmagick {
        elephc_imagick_set_quality($this->wand, $quality);
        return $this;
    }

    public function getCompressionQuality(): int {
        $_q = elephc_imagick_get_quality($this->wand);
        return $_q < 0 ? 0 : $_q;
    }

    public function getImageWidth(): int {
        return elephc_imagick_cur_width($this->wand);
    }

    public function getImageHeight(): int {
        return elephc_imagick_cur_height($this->wand);
    }

    // No return type hint so callers can read the "width"/"height" keys.
    public function getImageGeometry() {
        return ["width" => $this->getImageWidth(), "height" => $this->getImageHeight()];
    }

    public function resizeImage(int $width, int $height, int $filter, float $factor, bool $fit = false): Gmagick {
        $_u_filter = $filter;
        $_u_factor = $factor;
        $_u_fit = $fit;
        if (elephc_imagick_resize($this->wand, $width, $height) !== 0) {
            throw new GmagickException("Gmagick::resizeimage(): resize failed");
        }
        return $this;
    }

    public function scaleImage(int $width, int $height, bool $fit = false): Gmagick {
        $_u_fit = $fit;
        if (elephc_imagick_scale($this->wand, $width, $height) !== 0) {
            throw new GmagickException("Gmagick::scaleimage(): scale failed");
        }
        return $this;
    }

    public function thumbnailImage(int $width, int $height, bool $fit = false): Gmagick {
        $_u_fit = $fit;
        $_ow = $this->getImageWidth();
        $_oh = $this->getImageHeight();
        $_w = $width;
        $_h = $height;
        if ($width == 0 && $height > 0 && $_oh > 0) {
            $_w = (int) round($_ow * $height / $_oh);
        } elseif ($height == 0 && $width > 0 && $_ow > 0) {
            $_h = (int) round($_oh * $width / $_ow);
        }
        if ($_w < 1) { $_w = 1; }
        if ($_h < 1) { $_h = 1; }
        if (elephc_imagick_resize($this->wand, $_w, $_h) !== 0) {
            throw new GmagickException("Gmagick::thumbnailimage(): failed");
        }
        return $this;
    }

    public function cropImage(int $width, int $height, int $x, int $y): Gmagick {
        if (elephc_imagick_crop($this->wand, $width, $height, $x, $y) !== 0) {
            throw new GmagickException("Gmagick::cropimage(): invalid crop region");
        }
        return $this;
    }

    public function rotateImage($color, float $degrees): Gmagick {
        $_bg = _gmagick_norm_color($color);
        $_mdeg = (int) round($degrees * 1000);
        if (elephc_imagick_rotate($this->wand, $_mdeg, $_bg) !== 0) {
            throw new GmagickException("Gmagick::rotateimage(): rotate failed");
        }
        return $this;
    }

    public function flipImage(): Gmagick {
        if (elephc_imagick_flip($this->wand) !== 0) {
            throw new GmagickException("Gmagick::flipimage(): no image");
        }
        return $this;
    }

    public function flopImage(): Gmagick {
        if (elephc_imagick_flop($this->wand) !== 0) {
            throw new GmagickException("Gmagick::flopimage(): no image");
        }
        return $this;
    }

    public function blurImage(float $radius, float $sigma): Gmagick {
        $_sig = $sigma > 0.0 ? $sigma : $radius;
        if (elephc_imagick_blur($this->wand, (int) round($_sig * 1000)) !== 0) {
            throw new GmagickException("Gmagick::blurimage(): no image");
        }
        return $this;
    }

    public function gaussianBlurImage(float $radius, float $sigma): Gmagick {
        return $this->blurImage($radius, $sigma);
    }

    public function modulateImage(float $brightness, float $saturation, float $hue): Gmagick {
        if (elephc_imagick_modulate($this->wand, (int) round($brightness), (int) round($saturation), (int) round($hue)) !== 0) {
            throw new GmagickException("Gmagick::modulateimage(): no image");
        }
        return $this;
    }

    public function compositeImage(Gmagick $source, int $compose, int $x, int $y): Gmagick {
        $_rc = elephc_imagick_composite($this->wand, $source->_wandHandle(), $compose, $x, $y);
        if ($_rc === -2) {
            throw new GmagickException("Gmagick::compositeimage(): composite operator " . $compose . " is not supported in elephc");
        }
        if ($_rc !== 0) {
            throw new GmagickException("Gmagick::compositeimage(): composite failed");
        }
        return $this;
    }

    public function drawImage(GmagickDraw $draw): Gmagick {
        if (elephc_imagick_draw($this->wand, $draw->_gmagickHandle()) !== 0) {
            throw new GmagickException("Gmagick::drawimage(): draw failed");
        }
        return $this;
    }

    // elephc paints the background onto the current frame (no deferred slot);
    // documented difference from GraphicsMagick.
    public function setImageBackgroundColor($background): Gmagick {
        elephc_imagick_fill($this->wand, _gmagick_norm_color($background));
        return $this;
    }

    public function getNumberImages(): int {
        return elephc_imagick_count($this->wand);
    }

    public function getImageIndex(): int {
        return elephc_imagick_get_index($this->wand);
    }

    public function setImageIndex(int $index): Gmagick {
        elephc_imagick_set_index($this->wand, $index);
        return $this;
    }

    public function nextImage(): bool {
        return elephc_imagick_next($this->wand) === 1;
    }

    public function previousImage(): bool {
        return elephc_imagick_previous($this->wand) === 1;
    }

    public function hasNextImage(): bool {
        return ($this->getImageIndex() + 1) < $this->getNumberImages();
    }

    public function hasPreviousImage(): bool {
        return $this->getImageIndex() > 0;
    }

    // current() returns the wand positioned at the current frame.
    public function current(): Gmagick {
        return $this;
    }

    public function clear(): bool {
        elephc_imagick_clear($this->wand);
        return true;
    }

    public function destroy(): bool {
        elephc_imagick_destroy($this->wand);
        $this->wand = 0;
        return true;
    }

    public function __destruct() {
        elephc_imagick_destroy($this->wand);
    }

    // -- Version / package info --
    public function getCopyright(): string {
        return "elephc pure-Rust image bridge";
    }

    public function getPackageName(): string {
        return "elephc";
    }

    public function getReleaseDate(): string {
        return "";
    }

    public function getQuantumDepth(): array {
        return ["quantumDepthLong" => 8, "quantumString" => "Q8"];
    }

    // Returns the formats the pure-Rust codec bridge can read/write.
    public function queryFormats(string $pattern = "*"): array {
        $_u_pat = $pattern;
        return ["BMP", "GIF", "JPEG", "PNG", "WEBP"];
    }

    // -- Documented unsupported effects (no pure-Rust equivalent) --
    public function annotateImage(GmagickDraw $draw, float $x, float $y, float $angle, string $text): Gmagick {
        $_u_d = $draw;
        $_u_x = $x;
        $_u_y = $y;
        $_u_a = $angle;
        $_u_t = $text;
        throw new GmagickException("Gmagick::annotateimage() requires FreeType text, which is not supported in elephc");
    }

    public function charcoalImage(float $radius, float $sigma): Gmagick {
        $_u_r = $radius;
        $_u_s = $sigma;
        throw new GmagickException("Gmagick::charcoalimage() is not supported in elephc");
    }

    public function swirlImage(float $degrees): Gmagick {
        $_u_d = $degrees;
        throw new GmagickException("Gmagick::swirlimage() is not supported in elephc");
    }

    public function oilPaintImage(float $radius): Gmagick {
        $_u_r = $radius;
        throw new GmagickException("Gmagick::oilpaintimage() is not supported in elephc");
    }

    public function embossImage(float $radius, float $sigma): Gmagick {
        $_u_r = $radius;
        $_u_s = $sigma;
        throw new GmagickException("Gmagick::embossimage() is not supported in elephc");
    }
    // --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---
    public function addnoiseimage(int $noise_type): Gmagick {
        $_u_noise_type = $noise_type;
        throw new GmagickException("Gmagick::addnoiseimage() is not supported in elephc");
    }
    public function borderimage(GmagickPixel $color, int $width, int $height): Gmagick {
        $_u_color = $color;
        $_u_width = $width;
        $_u_height = $height;
        throw new GmagickException("Gmagick::borderimage() is not supported in elephc");
    }
    public function chopimage(int $width, int $height, int $x, int $y): Gmagick {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickException("Gmagick::chopimage() is not supported in elephc");
    }
    public function commentimage(string $comment): Gmagick {
        $_u_comment = $comment;
        throw new GmagickException("Gmagick::commentimage() is not supported in elephc");
    }
    public function cropthumbnailimage(int $width, int $height): Gmagick {
        $_u_width = $width;
        $_u_height = $height;
        throw new GmagickException("Gmagick::cropthumbnailimage() is not supported in elephc");
    }
    public function cyclecolormapimage(int $displace): Gmagick {
        $_u_displace = $displace;
        throw new GmagickException("Gmagick::cyclecolormapimage() is not supported in elephc");
    }
    public function deconstructimages(): Gmagick {
        throw new GmagickException("Gmagick::deconstructimages() is not supported in elephc");
    }
    public function despeckleimage(): Gmagick {
        throw new GmagickException("Gmagick::despeckleimage() is not supported in elephc");
    }
    public function edgeimage(float $radius): Gmagick {
        $_u_radius = $radius;
        throw new GmagickException("Gmagick::edgeimage() is not supported in elephc");
    }
    public function enhanceimage(): Gmagick {
        throw new GmagickException("Gmagick::enhanceimage() is not supported in elephc");
    }
    public function equalizeimage(): Gmagick {
        throw new GmagickException("Gmagick::equalizeimage() is not supported in elephc");
    }
    public function frameimage(GmagickPixel $color, int $width, int $height, int $inner_bevel, int $outer_bevel): Gmagick {
        $_u_color = $color;
        $_u_width = $width;
        $_u_height = $height;
        $_u_inner_bevel = $inner_bevel;
        $_u_outer_bevel = $outer_bevel;
        throw new GmagickException("Gmagick::frameimage() is not supported in elephc");
    }
    public function gammaimage(float $gamma): Gmagick {
        $_u_gamma = $gamma;
        throw new GmagickException("Gmagick::gammaimage() is not supported in elephc");
    }
    public function getfilename(): string {
        throw new GmagickException("Gmagick::getfilename() is not supported in elephc");
    }
    public function getimagebackgroundcolor(): GmagickPixel {
        throw new GmagickException("Gmagick::getimagebackgroundcolor() is not supported in elephc");
    }
    public function getimageblueprimary(): array {
        throw new GmagickException("Gmagick::getimageblueprimary() is not supported in elephc");
    }
    public function getimagebordercolor(): GmagickPixel {
        throw new GmagickException("Gmagick::getimagebordercolor() is not supported in elephc");
    }
    public function getimagechanneldepth(int $channel_type): int {
        $_u_channel_type = $channel_type;
        throw new GmagickException("Gmagick::getimagechanneldepth() is not supported in elephc");
    }
    public function getimagecolors(): int {
        throw new GmagickException("Gmagick::getimagecolors() is not supported in elephc");
    }
    public function getimagecolorspace(): int {
        throw new GmagickException("Gmagick::getimagecolorspace() is not supported in elephc");
    }
    public function getimagecompose(): int {
        throw new GmagickException("Gmagick::getimagecompose() is not supported in elephc");
    }
    public function getimagedelay(): int {
        throw new GmagickException("Gmagick::getimagedelay() is not supported in elephc");
    }
    public function getimagedepth(): int {
        throw new GmagickException("Gmagick::getimagedepth() is not supported in elephc");
    }
    public function getimagedispose(): int {
        throw new GmagickException("Gmagick::getimagedispose() is not supported in elephc");
    }
    public function getimageextrema(): array {
        throw new GmagickException("Gmagick::getimageextrema() is not supported in elephc");
    }
    public function getimagefilename(): string {
        throw new GmagickException("Gmagick::getimagefilename() is not supported in elephc");
    }
    public function getimagegamma(): float {
        throw new GmagickException("Gmagick::getimagegamma() is not supported in elephc");
    }
    public function getimagegreenprimary(): array {
        throw new GmagickException("Gmagick::getimagegreenprimary() is not supported in elephc");
    }
    public function getimagehistogram(): array {
        throw new GmagickException("Gmagick::getimagehistogram() is not supported in elephc");
    }
    public function getimageinterlacescheme(): int {
        throw new GmagickException("Gmagick::getimageinterlacescheme() is not supported in elephc");
    }
    public function getimageiterations(): int {
        throw new GmagickException("Gmagick::getimageiterations() is not supported in elephc");
    }
    public function getimagematte(): int {
        throw new GmagickException("Gmagick::getimagematte() is not supported in elephc");
    }
    public function getimagemattecolor(): GmagickPixel {
        throw new GmagickException("Gmagick::getimagemattecolor() is not supported in elephc");
    }
    public function getimageprofile(string $name): string {
        $_u_name = $name;
        throw new GmagickException("Gmagick::getimageprofile() is not supported in elephc");
    }
    public function getimageredprimary(): array {
        throw new GmagickException("Gmagick::getimageredprimary() is not supported in elephc");
    }
    public function getimagerenderingintent(): int {
        throw new GmagickException("Gmagick::getimagerenderingintent() is not supported in elephc");
    }
    public function getimageresolution(): array {
        throw new GmagickException("Gmagick::getimageresolution() is not supported in elephc");
    }
    public function getimagescene(): int {
        throw new GmagickException("Gmagick::getimagescene() is not supported in elephc");
    }
    public function getimagesignature(): string {
        throw new GmagickException("Gmagick::getimagesignature() is not supported in elephc");
    }
    public function getimagetype(): int {
        throw new GmagickException("Gmagick::getimagetype() is not supported in elephc");
    }
    public function getimageunits(): int {
        throw new GmagickException("Gmagick::getimageunits() is not supported in elephc");
    }
    public function getimagewhitepoint(): array {
        throw new GmagickException("Gmagick::getimagewhitepoint() is not supported in elephc");
    }
    public function getsamplingfactors(): array {
        throw new GmagickException("Gmagick::getsamplingfactors() is not supported in elephc");
    }
    public function getsize(): array {
        throw new GmagickException("Gmagick::getsize() is not supported in elephc");
    }
    public function getversion(): array {
        throw new GmagickException("Gmagick::getversion() is not supported in elephc");
    }
    public function implodeimage(float $radius): mixed {
        $_u_radius = $radius;
        throw new GmagickException("Gmagick::implodeimage() is not supported in elephc");
    }
    public function labelimage(string $label): mixed {
        $_u_label = $label;
        throw new GmagickException("Gmagick::labelimage() is not supported in elephc");
    }
    public function levelimage(float $blackPoint, float $gamma, float $whitePoint, int $channel = 0): mixed {
        $_u_blackPoint = $blackPoint;
        $_u_gamma = $gamma;
        $_u_whitePoint = $whitePoint;
        $_u_channel = $channel;
        throw new GmagickException("Gmagick::levelimage() is not supported in elephc");
    }
    public function magnifyimage(): mixed {
        throw new GmagickException("Gmagick::magnifyimage() is not supported in elephc");
    }
    public function mapimage(Gmagick $gmagick, bool $dither): Gmagick {
        $_u_gmagick = $gmagick;
        $_u_dither = $dither;
        throw new GmagickException("Gmagick::mapimage() is not supported in elephc");
    }
    public function medianfilterimage(float $radius): void {
        $_u_radius = $radius;
        throw new GmagickException("Gmagick::medianfilterimage() is not supported in elephc");
    }
    public function minifyimage(): Gmagick {
        throw new GmagickException("Gmagick::minifyimage() is not supported in elephc");
    }
    public function motionblurimage(float $radius, float $sigma, float $angle): Gmagick {
        $_u_radius = $radius;
        $_u_sigma = $sigma;
        $_u_angle = $angle;
        throw new GmagickException("Gmagick::motionblurimage() is not supported in elephc");
    }
    public function normalizeimage(int $channel = 0): Gmagick {
        $_u_channel = $channel;
        throw new GmagickException("Gmagick::normalizeimage() is not supported in elephc");
    }
    public function profileimage(string $name, string $profile): Gmagick {
        $_u_name = $name;
        $_u_profile = $profile;
        throw new GmagickException("Gmagick::profileimage() is not supported in elephc");
    }
    public function quantizeimage(int $numColors, int $colorspace, int $treeDepth, bool $dither, bool $measureError): Gmagick {
        $_u_numColors = $numColors;
        $_u_colorspace = $colorspace;
        $_u_treeDepth = $treeDepth;
        $_u_dither = $dither;
        $_u_measureError = $measureError;
        throw new GmagickException("Gmagick::quantizeimage() is not supported in elephc");
    }
    public function quantizeimages(int $numColors, int $colorspace, int $treeDepth, bool $dither, bool $measureError): Gmagick {
        $_u_numColors = $numColors;
        $_u_colorspace = $colorspace;
        $_u_treeDepth = $treeDepth;
        $_u_dither = $dither;
        $_u_measureError = $measureError;
        throw new GmagickException("Gmagick::quantizeimages() is not supported in elephc");
    }
    public function queryfontmetrics(GmagickDraw $draw, string $text): array {
        $_u_draw = $draw;
        $_u_text = $text;
        throw new GmagickException("Gmagick::queryfontmetrics() is not supported in elephc");
    }
    public function queryfonts(string $pattern = "*"): array {
        $_u_pattern = $pattern;
        throw new GmagickException("Gmagick::queryfonts() is not supported in elephc");
    }
    public function radialblurimage(float $angle, int $channel = 0): Gmagick {
        $_u_angle = $angle;
        $_u_channel = $channel;
        throw new GmagickException("Gmagick::radialblurimage() is not supported in elephc");
    }
    public function raiseimage(int $width, int $height, int $x, int $y, bool $raise): Gmagick {
        $_u_width = $width;
        $_u_height = $height;
        $_u_x = $x;
        $_u_y = $y;
        $_u_raise = $raise;
        throw new GmagickException("Gmagick::raiseimage() is not supported in elephc");
    }
    public function read(string $filename): Gmagick {
        $_u_filename = $filename;
        throw new GmagickException("Gmagick::read() is not supported in elephc");
    }
    public function readimagefile($fp, string $filename = ""): Gmagick {
        $_u_fp = $fp;
        $_u_filename = $filename;
        throw new GmagickException("Gmagick::readimagefile() is not supported in elephc");
    }
    public function reducenoiseimage(float $radius): Gmagick {
        $_u_radius = $radius;
        throw new GmagickException("Gmagick::reducenoiseimage() is not supported in elephc");
    }
    public function removeimage(): Gmagick {
        throw new GmagickException("Gmagick::removeimage() is not supported in elephc");
    }
    public function removeimageprofile(string $name): string {
        $_u_name = $name;
        throw new GmagickException("Gmagick::removeimageprofile() is not supported in elephc");
    }
    public function resampleimage(float $xResolution, float $yResolution, int $filter, float $blur): Gmagick {
        $_u_xResolution = $xResolution;
        $_u_yResolution = $yResolution;
        $_u_filter = $filter;
        $_u_blur = $blur;
        throw new GmagickException("Gmagick::resampleimage() is not supported in elephc");
    }
    public function rollimage(int $x, int $y): Gmagick {
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickException("Gmagick::rollimage() is not supported in elephc");
    }
    public function separateimagechannel(int $channel): Gmagick {
        $_u_channel = $channel;
        throw new GmagickException("Gmagick::separateimagechannel() is not supported in elephc");
    }
    public function setfilename(string $filename): Gmagick {
        $_u_filename = $filename;
        throw new GmagickException("Gmagick::setfilename() is not supported in elephc");
    }
    public function setimageblueprimary(float $x, float $y): Gmagick {
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickException("Gmagick::setimageblueprimary() is not supported in elephc");
    }
    public function setimagebordercolor(GmagickPixel $color): Gmagick {
        $_u_color = $color;
        throw new GmagickException("Gmagick::setimagebordercolor() is not supported in elephc");
    }
    public function setimagechanneldepth(int $channel, int $depth): Gmagick {
        $_u_channel = $channel;
        $_u_depth = $depth;
        throw new GmagickException("Gmagick::setimagechanneldepth() is not supported in elephc");
    }
    public function setimagecolorspace(int $colorspace): Gmagick {
        $_u_colorspace = $colorspace;
        throw new GmagickException("Gmagick::setimagecolorspace() is not supported in elephc");
    }
    public function setimagecompose(int $composite): Gmagick {
        $_u_composite = $composite;
        throw new GmagickException("Gmagick::setimagecompose() is not supported in elephc");
    }
    public function setimagedelay(int $delay): Gmagick {
        $_u_delay = $delay;
        throw new GmagickException("Gmagick::setimagedelay() is not supported in elephc");
    }
    public function setimagedepth(int $depth): Gmagick {
        $_u_depth = $depth;
        throw new GmagickException("Gmagick::setimagedepth() is not supported in elephc");
    }
    public function setimagedispose(int $disposeType): Gmagick {
        $_u_disposeType = $disposeType;
        throw new GmagickException("Gmagick::setimagedispose() is not supported in elephc");
    }
    public function setimagefilename(string $filename): Gmagick {
        $_u_filename = $filename;
        throw new GmagickException("Gmagick::setimagefilename() is not supported in elephc");
    }
    public function setimagegamma(float $gamma): Gmagick {
        $_u_gamma = $gamma;
        throw new GmagickException("Gmagick::setimagegamma() is not supported in elephc");
    }
    public function setimagegreenprimary(float $x, float $y): Gmagick {
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickException("Gmagick::setimagegreenprimary() is not supported in elephc");
    }
    public function setimageinterlacescheme(int $interlace): Gmagick {
        $_u_interlace = $interlace;
        throw new GmagickException("Gmagick::setimageinterlacescheme() is not supported in elephc");
    }
    public function setimageiterations(int $iterations): Gmagick {
        $_u_iterations = $iterations;
        throw new GmagickException("Gmagick::setimageiterations() is not supported in elephc");
    }
    public function setimageprofile(string $name, string $profile): Gmagick {
        $_u_name = $name;
        $_u_profile = $profile;
        throw new GmagickException("Gmagick::setimageprofile() is not supported in elephc");
    }
    public function setimageredprimary(float $x, float $y): Gmagick {
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickException("Gmagick::setimageredprimary() is not supported in elephc");
    }
    public function setimagerenderingintent(int $rendering_intent): Gmagick {
        $_u_rendering_intent = $rendering_intent;
        throw new GmagickException("Gmagick::setimagerenderingintent() is not supported in elephc");
    }
    public function setimageresolution(float $xResolution, float $yResolution): Gmagick {
        $_u_xResolution = $xResolution;
        $_u_yResolution = $yResolution;
        throw new GmagickException("Gmagick::setimageresolution() is not supported in elephc");
    }
    public function setimagescene(int $scene): Gmagick {
        $_u_scene = $scene;
        throw new GmagickException("Gmagick::setimagescene() is not supported in elephc");
    }
    public function setimagetype(int $imgType): Gmagick {
        $_u_imgType = $imgType;
        throw new GmagickException("Gmagick::setimagetype() is not supported in elephc");
    }
    public function setimageunits(int $resolution): Gmagick {
        $_u_resolution = $resolution;
        throw new GmagickException("Gmagick::setimageunits() is not supported in elephc");
    }
    public function setimagewhitepoint(float $x, float $y): Gmagick {
        $_u_x = $x;
        $_u_y = $y;
        throw new GmagickException("Gmagick::setimagewhitepoint() is not supported in elephc");
    }
    public function setsamplingfactors(array $factors): Gmagick {
        $_u_factors = $factors;
        throw new GmagickException("Gmagick::setsamplingfactors() is not supported in elephc");
    }
    public function setsize(int $columns, int $rows): Gmagick {
        $_u_columns = $columns;
        $_u_rows = $rows;
        throw new GmagickException("Gmagick::setsize() is not supported in elephc");
    }
    public function shearimage($color, float $xShear, float $yShear): Gmagick {
        $_u_color = $color;
        $_u_xShear = $xShear;
        $_u_yShear = $yShear;
        throw new GmagickException("Gmagick::shearimage() is not supported in elephc");
    }
    public function solarizeimage(int $threshold): Gmagick {
        $_u_threshold = $threshold;
        throw new GmagickException("Gmagick::solarizeimage() is not supported in elephc");
    }
    public function spreadimage(float $radius): Gmagick {
        $_u_radius = $radius;
        throw new GmagickException("Gmagick::spreadimage() is not supported in elephc");
    }
    public function stripimage(): Gmagick {
        throw new GmagickException("Gmagick::stripimage() is not supported in elephc");
    }
    public function trimimage(float $fuzz): Gmagick {
        $_u_fuzz = $fuzz;
        throw new GmagickException("Gmagick::trimimage() is not supported in elephc");
    }
    // --- end auto-generated API-surface stubs ---
}

// ===========================================================================
// Cairo OOP. Vector drawing on the pure-Rust tiny-skia bridge
// (elephc_cairo_* externs). CairoImageSurface (PNG) is fully supported; PDF/PS/SVG
// surfaces and FreeType text are documented gaps that throw CairoException.
// Geometry crosses the bridge as fixed-point milli-units packed via the shared
// _imagick_pack2; colors as packed RGBA8. Numeric params are untyped so int and
// float coordinates are both accepted (cast inside the _cairo_* helpers).
// ---------------------------------------------------------------------------

class CairoException extends Exception {
}

class CairoFormat {
    const ARGB32 = 0;
    const RGB24 = 1;
    const A8 = 2;
    const A1 = 3;
}

class CairoAntialias {
    const DEFAULT = 0;
    const NONE = 1;
    const GRAY = 2;
    const SUBPIXEL = 3;
}

class CairoLineCap {
    const BUTT = 0;
    const ROUND = 1;
    const SQUARE = 2;
}

class CairoLineJoin {
    const MITER = 0;
    const ROUND = 1;
    const BEVEL = 2;
}

class CairoFillRule {
    const WINDING = 0;
    const EVEN_ODD = 1;
}

class CairoFontSlant {
    const NORMAL = 0;
    const ITALIC = 1;
    const OBLIQUE = 2;
}

class CairoFontWeight {
    const NORMAL = 0;
    const BOLD = 1;
}

// Converts a number to fixed-point milli-units (value * 1000) for the bridge.
function _cairo_fx($v): int {
    return (int) round(((float) $v) * 1000.0);
}

// Packs two numbers as a fixed-point (x, y) pair into one int.
function _cairo_pack($x, $y): int {
    return _imagick_pack2(_cairo_fx($x), _cairo_fx($y));
}

// Clamps a 0..255 channel value into range.
function _cairo_clamp8(int $v): int {
    if ($v < 0) {
        return 0;
    }
    if ($v > 255) {
        return 255;
    }
    return $v;
}

// Packs four 0..1 color components into a single RGBA8 integer.
function _cairo_color($r, $g, $b, $a): int {
    $_ri = _cairo_clamp8((int) round(((float) $r) * 255.0));
    $_gi = _cairo_clamp8((int) round(((float) $g) * 255.0));
    $_bi = _cairo_clamp8((int) round(((float) $b) * 255.0));
    $_ai = _cairo_clamp8((int) round(((float) $a) * 255.0));
    return ($_ri << 24) | ($_gi << 16) | ($_bi << 8) | $_ai;
}

class CairoSurface {
}

class CairoImageSurface extends CairoSurface {
    public int $surface = 0;

    public function __construct(int $format, int $width, int $height) {
        $_u_fmt = $format;
        $this->surface = elephc_cairo_surface_create($width, $height);
        if ($this->surface < 0) {
            throw new CairoException("CairoImageSurface: invalid dimensions");
        }
    }

    // Internal: exposes the bridge surface handle to CairoContext.
    public function _surfaceHandle(): int {
        return $this->surface;
    }

    public function getWidth(): int {
        return elephc_cairo_surface_width($this->surface);
    }

    public function getHeight(): int {
        return elephc_cairo_surface_height($this->surface);
    }

    public function getFormat(): int {
        return 0;
    }

    public function status(): int {
        return 0;
    }

    public function flush(): void {
    }

    public function finish(): void {
    }

    public function writeToPng(string $file): void {
        if (elephc_cairo_surface_write_png($this->surface, $file) !== 0) {
            throw new CairoException("CairoImageSurface::writeToPng(): unable to write '" . $file . "'");
        }
    }

    /// Loads a PNG file into a new image surface (alpha is premultiplied by the
    /// bridge). The constructor only builds blank surfaces, so this adopts the
    /// decoded handle by discarding the blank one the constructor allocates.
    public static function createFromPng(string $file): CairoImageSurface {
        $h = elephc_cairo_surface_create_from_png($file);
        if ($h < 0) {
            throw new CairoException("CairoImageSurface::createFromPng(): unable to load '" . $file . "'");
        }
        $s = new CairoImageSurface(CairoFormat::ARGB32, 1, 1);
        elephc_cairo_surface_destroy($s->surface);
        $s->surface = $h;
        return $s;
    }

    public function __destruct() {
        elephc_cairo_surface_destroy($this->surface);
    }
}

class CairoPdfSurface extends CairoSurface {
    public function __construct(string $file, $width, $height) {
        $_u_f = $file;
        $_u_w = $width;
        $_u_h = $height;
        throw new CairoException("CairoPdfSurface is not supported in elephc (no pure-Rust PDF surface)");
    }
}

class CairoPsSurface extends CairoSurface {
    public function __construct(string $file, $width, $height) {
        $_u_f = $file;
        $_u_w = $width;
        $_u_h = $height;
        throw new CairoException("CairoPsSurface is not supported in elephc (no pure-Rust PostScript surface)");
    }
}

class CairoSvgSurface extends CairoSurface {
    public function __construct(string $file, $width, $height) {
        $_u_f = $file;
        $_u_w = $width;
        $_u_h = $height;
        throw new CairoException("CairoSvgSurface is not supported in elephc (no pure-Rust SVG surface)");
    }
}

class CairoPattern {
    public int $pattern = 0;

    // Internal: exposes the bridge pattern handle to CairoContext::setSource.
    public function _patternHandle(): int {
        return $this->pattern;
    }

    public function status(): int {
        return 0;
    }

    public function __destruct() {
        elephc_cairo_pattern_destroy($this->pattern);
    }
}

class CairoSolidPattern extends CairoPattern {
    public function __construct(float $r, float $g, float $b, float $a = 1.0) {
        $this->pattern = elephc_cairo_pattern_create_rgba(_cairo_color($r, $g, $b, $a));
    }

    public static function createRgb(float $r, float $g, float $b): CairoSolidPattern {
        return new CairoSolidPattern($r, $g, $b, 1.0);
    }

    public static function createRgba(float $r, float $g, float $b, float $a): CairoSolidPattern {
        return new CairoSolidPattern($r, $g, $b, $a);
    }
}

class CairoGradientPattern extends CairoPattern {
    public function addColorStopRgb(float $offset, float $r, float $g, float $b): void {
        elephc_cairo_pattern_add_color_stop_rgba($this->pattern, _cairo_fx($offset), _cairo_color($r, $g, $b, 1.0));
    }

    public function addColorStopRgba(float $offset, float $r, float $g, float $b, float $a): void {
        elephc_cairo_pattern_add_color_stop_rgba($this->pattern, _cairo_fx($offset), _cairo_color($r, $g, $b, $a));
    }
}

class CairoLinearGradient extends CairoGradientPattern {
    public function __construct(float $x0, float $y0, float $x1, float $y1) {
        $this->pattern = elephc_cairo_pattern_create_linear(_cairo_pack($x0, $y0), _cairo_pack($x1, $y1));
    }
}

class CairoRadialGradient extends CairoGradientPattern {
    public function __construct(float $cx0, float $cy0, float $radius0, float $cx1, float $cy1, float $radius1) {
        $this->pattern = elephc_cairo_pattern_create_radial(
            _cairo_pack($cx0, $cy0),
            _cairo_fx($radius0),
            _cairo_pack($cx1, $cy1),
            _cairo_fx($radius1)
        );
    }
}

class CairoSurfacePattern extends CairoPattern {
    public function __construct(CairoImageSurface $surface) {
        $_u = $surface;
        throw new CairoException("CairoSurfacePattern is not supported in elephc");
    }
}

class CairoMatrix {
    public float $xx = 1.0;
    public float $yx = 0.0;
    public float $xy = 0.0;
    public float $yy = 1.0;
    public float $x0 = 0.0;
    public float $y0 = 0.0;

    public function __construct(float $xx = 1.0, float $yx = 0.0, float $xy = 0.0, float $yy = 1.0, float $x0 = 0.0, float $y0 = 0.0) {
        $this->xx = $xx;
        $this->yx = $yx;
        $this->xy = $xy;
        $this->yy = $yy;
        $this->x0 = $x0;
        $this->y0 = $y0;
    }

    public function initIdentity(): void {
        $this->xx = 1.0;
        $this->yx = 0.0;
        $this->xy = 0.0;
        $this->yy = 1.0;
        $this->x0 = 0.0;
        $this->y0 = 0.0;
    }

    public function initTranslate(float $tx, float $ty): void {
        $this->xx = 1.0;
        $this->yx = 0.0;
        $this->xy = 0.0;
        $this->yy = 1.0;
        $this->x0 = $tx;
        $this->y0 = $ty;
    }

    public function initScale(float $sx, float $sy): void {
        $this->xx = $sx;
        $this->yx = 0.0;
        $this->xy = 0.0;
        $this->yy = $sy;
        $this->x0 = 0.0;
        $this->y0 = 0.0;
    }

    public function initRotate(float $radians): void {
        $_c = cos($radians);
        $_s = sin($radians);
        $this->xx = $_c;
        $this->yx = $_s;
        $this->xy = -$_s;
        $this->yy = $_c;
        $this->x0 = 0.0;
        $this->y0 = 0.0;
    }

    // Applies the matrix to a point, returning ["x" => , "y" => ].
    public function transformPoint(float $x, float $y): array {
        $_fx = $x;
        $_fy = $y;
        return [
            "x" => $this->xx * $_fx + $this->xy * $_fy + $this->x0,
            "y" => $this->yx * $_fx + $this->yy * $_fy + $this->y0,
        ];
    }
}

class CairoContext {
    public int $ctx = 0;

    public function __construct(CairoImageSurface $surface) {
        $this->ctx = elephc_cairo_create($surface->_surfaceHandle());
        if ($this->ctx < 0) {
            throw new CairoException("CairoContext: invalid surface");
        }
    }

    public function save(): void {
        elephc_cairo_save($this->ctx);
    }

    public function restore(): void {
        elephc_cairo_restore($this->ctx);
    }

    public function setSourceRgb(float $r, float $g, float $b): void {
        elephc_cairo_set_source_rgba($this->ctx, _cairo_color($r, $g, $b, 1.0));
    }

    public function setSourceRgba(float $r, float $g, float $b, float $a): void {
        elephc_cairo_set_source_rgba($this->ctx, _cairo_color($r, $g, $b, $a));
    }

    public function setSource(CairoPattern $pattern): void {
        elephc_cairo_set_source_pattern($this->ctx, $pattern->_patternHandle());
    }

    public function setLineWidth(float $width): void {
        elephc_cairo_set_line_width($this->ctx, _cairo_fx($width));
    }

    public function setLineCap(int $cap): void {
        elephc_cairo_set_line_cap($this->ctx, $cap);
    }

    public function setLineJoin(int $join): void {
        elephc_cairo_set_line_join($this->ctx, $join);
    }

    public function setFillRule(int $rule): void {
        elephc_cairo_set_fill_rule($this->ctx, $rule);
    }

    public function moveTo(float $x, float $y): void {
        elephc_cairo_move_to($this->ctx, _cairo_pack($x, $y));
    }

    public function lineTo(float $x, float $y): void {
        elephc_cairo_line_to($this->ctx, _cairo_pack($x, $y));
    }

    public function curveTo(float $x1, float $y1, float $x2, float $y2, float $x3, float $y3): void {
        elephc_cairo_curve_to($this->ctx, _cairo_pack($x1, $y1), _cairo_pack($x2, $y2), _cairo_pack($x3, $y3));
    }

    public function rectangle(float $x, float $y, float $width, float $height): void {
        elephc_cairo_rectangle($this->ctx, _cairo_pack($x, $y), _cairo_pack($width, $height));
    }

    public function arc(float $xc, float $yc, float $radius, float $angle1, float $angle2): void {
        elephc_cairo_arc($this->ctx, _cairo_pack($xc, $yc), _cairo_fx($radius), _cairo_pack($angle1, $angle2));
    }

    public function arcNegative(float $xc, float $yc, float $radius, float $angle1, float $angle2): void {
        elephc_cairo_arc_negative($this->ctx, _cairo_pack($xc, $yc), _cairo_fx($radius), _cairo_pack($angle1, $angle2));
    }

    public function closePath(): void {
        elephc_cairo_close_path($this->ctx);
    }

    public function newPath(): void {
        elephc_cairo_new_path($this->ctx);
    }

    public function newSubPath(): void {
        elephc_cairo_new_sub_path($this->ctx);
    }

    public function paint(): void {
        elephc_cairo_paint($this->ctx);
    }

    public function fill(): void {
        elephc_cairo_fill($this->ctx);
    }

    public function fillPreserve(): void {
        elephc_cairo_fill_preserve($this->ctx);
    }

    public function stroke(): void {
        elephc_cairo_stroke($this->ctx);
    }

    public function strokePreserve(): void {
        elephc_cairo_stroke_preserve($this->ctx);
    }

    public function translate(float $tx, float $ty): void {
        elephc_cairo_translate($this->ctx, _cairo_pack($tx, $ty));
    }

    public function scale(float $sx, float $sy): void {
        elephc_cairo_scale($this->ctx, _cairo_pack($sx, $sy));
    }

    public function rotate(float $angle): void {
        elephc_cairo_rotate($this->ctx, _cairo_fx($angle));
    }

    public function setMatrix(CairoMatrix $matrix): void {
        elephc_cairo_set_matrix(
            $this->ctx,
            _cairo_pack($matrix->xx, $matrix->yx),
            _cairo_pack($matrix->xy, $matrix->yy),
            _cairo_pack($matrix->x0, $matrix->y0)
        );
    }

    public function transform(CairoMatrix $matrix): void {
        elephc_cairo_transform(
            $this->ctx,
            _cairo_pack($matrix->xx, $matrix->yx),
            _cairo_pack($matrix->xy, $matrix->yy),
            _cairo_pack($matrix->x0, $matrix->y0)
        );
    }

    public function identityMatrix(): void {
        elephc_cairo_identity_matrix($this->ctx);
    }

    // Returns the current point as ["x" => , "y" => ] in user/device units.
    public function getCurrentPoint(): array {
        return [
            "x" => elephc_cairo_get_current_point_x($this->ctx) / 1000.0,
            "y" => elephc_cairo_get_current_point_y($this->ctx) / 1000.0,
        ];
    }

    // -- FreeType text: documented gaps. Font setup is a no-op so non-text
    //    drawing is unaffected; the actual rendering calls throw. --
    public function selectFontFace(string $family, int $slant = 0, int $weight = 0): void {
        $_u_f = $family;
        $_u_s = $slant;
        $_u_w = $weight;
    }

    public function setFontSize(float $size): void {
        $_u = $size;
    }

    public function showText(string $text): void {
        $_u = $text;
        throw new CairoException("CairoContext::showText() requires FreeType, which is not supported in elephc");
    }

    public function textExtents(string $text): array {
        $_u = $text;
        throw new CairoException("CairoContext::textExtents() requires FreeType, which is not supported in elephc");
    }

    public function __destruct() {
        elephc_cairo_destroy($this->ctx);
    }
}

class CairoFontFace {
    public function status(): int {
        return 0;
    }
}

class CairoToyFontFace extends CairoFontFace {
    public function __construct(string $family, int $slant = 0, int $weight = 0) {
        $_u_f = $family;
        $_u_s = $slant;
        $_u_w = $weight;
        throw new CairoException("CairoToyFontFace requires FreeType, which is not supported in elephc");
    }
}

class CairoFontOptions {
    public function status(): int {
        return 0;
    }
}

class CairoScaledFont {
    public function __construct(CairoFontFace $fontFace, CairoMatrix $matrix, CairoMatrix $ctm, CairoFontOptions $options) {
        $_u_a = $fontFace;
        $_u_b = $matrix;
        $_u_c = $ctm;
        $_u_d = $options;
        throw new CairoException("CairoScaledFont requires FreeType, which is not supported in elephc");
    }
}

class CairoPath {
}

// ===========================================================================
// Procedural cairo_* API (common subset). Thin wrappers over the
// Cairo* OOP classes mirroring the PECL cairo functional layer for the surface,
// context, pattern, and matrix operations that have a pure-Rust path. Each
// delegates to the matching OOP method so the two layers never drift apart.
// Obscure PECL functions with no pure-Rust path (font options, scaled fonts,
// PDF/PS/SVG surface internals, surface device offsets, ...) are intentionally
// omitted; calling one yields a normal "undefined function" error rather than
// a silent stub. Geometry and colors use plain PHP floats (0..1) like the OOP
// API; the helpers below pack them for the bridge.
// ---------------------------------------------------------------------------

// -- surfaces --
function cairo_image_surface_create(int $format, int $width, int $height): CairoImageSurface {
    return new CairoImageSurface($format, $width, $height);
}

function cairo_image_surface_create_from_png(string $filename): CairoImageSurface {
    return CairoImageSurface::createFromPng($filename);
}

function cairo_image_surface_get_width(CairoImageSurface $surface): int {
    return $surface->getWidth();
}

function cairo_image_surface_get_height(CairoImageSurface $surface): int {
    return $surface->getHeight();
}

function cairo_surface_write_to_png(CairoImageSurface $surface, string $filename): void {
    $surface->writeToPng($filename);
}

// -- context creation + save/restore --
function cairo_create(CairoImageSurface $surface): CairoContext {
    return new CairoContext($surface);
}

function cairo_save(CairoContext $context): void {
    $context->save();
}

function cairo_restore(CairoContext $context): void {
    $context->restore();
}

// -- source --
function cairo_set_source_rgb(CairoContext $context, float $red, float $green, float $blue): void {
    $context->setSourceRgb($red, $green, $blue);
}

function cairo_set_source_rgba(CairoContext $context, float $red, float $green, float $blue, float $alpha): void {
    $context->setSourceRgba($red, $green, $blue, $alpha);
}

function cairo_set_source(CairoContext $context, CairoPattern $pattern): void {
    $context->setSource($pattern);
}

// -- line state --
function cairo_set_line_width(CairoContext $context, float $width): void {
    $context->setLineWidth($width);
}

function cairo_set_line_cap(CairoContext $context, int $lineCap): void {
    $context->setLineCap($lineCap);
}

function cairo_set_line_join(CairoContext $context, int $lineJoin): void {
    $context->setLineJoin($lineJoin);
}

function cairo_set_fill_rule(CairoContext $context, int $fillRule): void {
    $context->setFillRule($fillRule);
}

// -- path construction --
function cairo_move_to(CairoContext $context, float $x, float $y): void {
    $context->moveTo($x, $y);
}

function cairo_line_to(CairoContext $context, float $x, float $y): void {
    $context->lineTo($x, $y);
}

function cairo_curve_to(CairoContext $context, float $x1, float $y1, float $x2, float $y2, float $x3, float $y3): void {
    $context->curveTo($x1, $y1, $x2, $y2, $x3, $y3);
}

function cairo_rectangle(CairoContext $context, float $x, float $y, float $width, float $height): void {
    $context->rectangle($x, $y, $width, $height);
}

function cairo_arc(CairoContext $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void {
    $context->arc($xc, $yc, $radius, $angle1, $angle2);
}

function cairo_arc_negative(CairoContext $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void {
    $context->arcNegative($xc, $yc, $radius, $angle1, $angle2);
}

function cairo_close_path(CairoContext $context): void {
    $context->closePath();
}

function cairo_new_path(CairoContext $context): void {
    $context->newPath();
}

function cairo_new_sub_path(CairoContext $context): void {
    $context->newSubPath();
}

// -- rendering --
function cairo_paint(CairoContext $context): void {
    $context->paint();
}

function cairo_fill(CairoContext $context): void {
    $context->fill();
}

function cairo_fill_preserve(CairoContext $context): void {
    $context->fillPreserve();
}

function cairo_stroke(CairoContext $context): void {
    $context->stroke();
}

function cairo_stroke_preserve(CairoContext $context): void {
    $context->strokePreserve();
}

// -- transforms --
function cairo_translate(CairoContext $context, float $tx, float $ty): void {
    $context->translate($tx, $ty);
}

function cairo_scale(CairoContext $context, float $sx, float $sy): void {
    $context->scale($sx, $sy);
}

function cairo_rotate(CairoContext $context, float $angle): void {
    $context->rotate($angle);
}

function cairo_set_matrix(CairoContext $context, CairoMatrix $matrix): void {
    $context->setMatrix($matrix);
}

function cairo_transform(CairoContext $context, CairoMatrix $matrix): void {
    $context->transform($matrix);
}

function cairo_identity_matrix(CairoContext $context): void {
    $context->identityMatrix();
}

// Returns the current point as an ["x" => , "y" => ] assoc. Inlined from the
// OOP body (rather than delegating) because a call to getCurrentPoint() inside
// the prelude carries the declared `array` static type, which forces either an
// unsupported AssocArray→Array(Mixed) conversion at the return or an int-keyed
// local for any intermediate; building the assoc literal here mirrors the OOP
// method exactly and avoids both.
function cairo_get_current_point(CairoContext $context): array {
    return [
        "x" => elephc_cairo_get_current_point_x($context->ctx) / 1000.0,
        "y" => elephc_cairo_get_current_point_y($context->ctx) / 1000.0,
    ];
}

// -- patterns --
function cairo_pattern_create_rgba(float $red, float $green, float $blue, float $alpha): CairoSolidPattern {
    return new CairoSolidPattern($red, $green, $blue, $alpha);
}

function cairo_pattern_create_rgb(float $red, float $green, float $blue): CairoSolidPattern {
    return CairoSolidPattern::createRgb($red, $green, $blue);
}

function cairo_pattern_create_linear(float $x0, float $y0, float $x1, float $y1): CairoLinearGradient {
    return new CairoLinearGradient($x0, $y0, $x1, $y1);
}

function cairo_pattern_create_radial(float $cx0, float $cy0, float $radius0, float $cx1, float $cy1, float $radius1): CairoRadialGradient {
    return new CairoRadialGradient($cx0, $cy0, $radius0, $cx1, $cy1, $radius1);
}

function cairo_pattern_add_color_stop_rgb(CairoGradientPattern $pattern, float $offset, float $red, float $green, float $blue): void {
    $pattern->addColorStopRgb($offset, $red, $green, $blue);
}

function cairo_pattern_add_color_stop_rgba(CairoGradientPattern $pattern, float $offset, float $red, float $green, float $blue, float $alpha): void {
    $pattern->addColorStopRgba($offset, $red, $green, $blue, $alpha);
}

// -- matrices (value objects) --
function cairo_matrix_init_identity(): CairoMatrix {
    return new CairoMatrix();
}

function cairo_matrix_init_translate(float $tx, float $ty): CairoMatrix {
    return new CairoMatrix(1.0, 0.0, 0.0, 1.0, $tx, $ty);
}

function cairo_matrix_init_scale(float $sx, float $sy): CairoMatrix {
    return new CairoMatrix($sx, 0.0, 0.0, $sy, 0.0, 0.0);
}

function cairo_matrix_init_rotate(float $radians): CairoMatrix {
    $m = new CairoMatrix();
    $m->initRotate($radians);
    return $m;
}

// Composes two Cairo matrices (m2 applied first, then m1) using the Cairo affine
// convention [xx yx xy yy x0 y0].
function cairo_matrix_multiply(CairoMatrix $m1, CairoMatrix $m2): CairoMatrix {
    return new CairoMatrix(
        $m1->xx * $m2->xx + $m1->xy * $m2->yx,
        $m1->yx * $m2->xx + $m1->yy * $m2->yx,
        $m1->xx * $m2->xy + $m1->xy * $m2->yy,
        $m1->yx * $m2->xy + $m1->yy * $m2->yy,
        $m1->xx * $m2->x0 + $m1->xy * $m2->y0 + $m1->x0,
        $m1->yx * $m2->x0 + $m1->yy * $m2->y0 + $m1->y0
    );
}

// Applies the matrix to a point, returning ["x" => , "y" => ]. Inlined from the
// OOP body for the same reason as cairo_get_current_point above.
function cairo_matrix_transform_point(CairoMatrix $matrix, float $x, float $y): array {
    return [
        "x" => $matrix->xx * $x + $matrix->xy * $y + $matrix->x0,
        "y" => $matrix->yx * $x + $matrix->yy * $y + $matrix->y0,
    ];
}
"#;

/// Prepends the image prelude to `program` when it references an image symbol, so
/// the classes, constants, functions, and `elephc_image` externs compile through
/// the normal pipeline only for image-using programs. The prelude carries only
/// declarations (extern block + const + class + functions), which are hoisted, so
/// prepending them ahead of user code does not change top-level execution order.
/// The prelude is static and tested, so a tokenize/parse failure is a compiler
/// bug and panics rather than silently degrading.
pub fn inject_if_used(program: crate::parser::ast::Program) -> crate::parser::ast::Program {
    if !detect::program_uses_image(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(IMAGE_PRELUDE_SRC).expect("image prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("image prelude must parse");
    combined.extend(program);
    combined
}
