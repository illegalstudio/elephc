---
title: "Image builtins"
description: "Builtins in the Image category."
sidebar:
  order: 121
---

## Image builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`cairo_arc()`](./image/cairo_arc.md) | `(mixed $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void` | `void` | ✓ | — |
| [`cairo_arc_negative()`](./image/cairo_arc_negative.md) | `(mixed $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void` | `void` | ✓ | — |
| [`cairo_close_path()`](./image/cairo_close_path.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_create()`](./image/cairo_create.md) | `(mixed $surface): mixed` | `mixed` | ✓ | — |
| [`cairo_curve_to()`](./image/cairo_curve_to.md) | `(mixed $context, float $x1, float $y1, float $x2, float $y2, float $x3, float $y3): void` | `void` | ✓ | — |
| [`cairo_fill()`](./image/cairo_fill.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_fill_preserve()`](./image/cairo_fill_preserve.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_get_current_point()`](./image/cairo_get_current_point.md) | `(mixed $context): mixed` | `mixed` | ✓ | — |
| [`cairo_identity_matrix()`](./image/cairo_identity_matrix.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_image_surface_create()`](./image/cairo_image_surface_create.md) | `(int $format, int $width, int $height): mixed` | `mixed` | ✓ | — |
| [`cairo_image_surface_create_from_png()`](./image/cairo_image_surface_create_from_png.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`cairo_image_surface_get_height()`](./image/cairo_image_surface_get_height.md) | `(mixed $surface): int` | `int` | ✓ | — |
| [`cairo_image_surface_get_width()`](./image/cairo_image_surface_get_width.md) | `(mixed $surface): int` | `int` | ✓ | — |
| [`cairo_line_to()`](./image/cairo_line_to.md) | `(mixed $context, float $x, float $y): void` | `void` | ✓ | — |
| [`cairo_matrix_init_identity()`](./image/cairo_matrix_init_identity.md) | `(): mixed` | `mixed` | ✓ | — |
| [`cairo_matrix_init_rotate()`](./image/cairo_matrix_init_rotate.md) | `(float $radians): mixed` | `mixed` | ✓ | — |
| [`cairo_matrix_init_scale()`](./image/cairo_matrix_init_scale.md) | `(float $sx, float $sy): mixed` | `mixed` | ✓ | — |
| [`cairo_matrix_init_translate()`](./image/cairo_matrix_init_translate.md) | `(float $tx, float $ty): mixed` | `mixed` | ✓ | — |
| [`cairo_matrix_multiply()`](./image/cairo_matrix_multiply.md) | `(mixed $m1, mixed $m2): mixed` | `mixed` | ✓ | — |
| [`cairo_matrix_transform_point()`](./image/cairo_matrix_transform_point.md) | `(mixed $matrix, float $x, float $y): mixed` | `mixed` | ✓ | — |
| [`cairo_move_to()`](./image/cairo_move_to.md) | `(mixed $context, float $x, float $y): void` | `void` | ✓ | — |
| [`cairo_new_path()`](./image/cairo_new_path.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_new_sub_path()`](./image/cairo_new_sub_path.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_paint()`](./image/cairo_paint.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_pattern_add_color_stop_rgb()`](./image/cairo_pattern_add_color_stop_rgb.md) | `(mixed $pattern, float $offset, float $red, float $green, float $blue): void` | `void` | ✓ | — |
| [`cairo_pattern_add_color_stop_rgba()`](./image/cairo_pattern_add_color_stop_rgba.md) | `(mixed $pattern, float $offset, float $red, float $green, float $blue, float $alpha): void` | `void` | ✓ | — |
| [`cairo_pattern_create_linear()`](./image/cairo_pattern_create_linear.md) | `(float $x0, float $y0, float $x1, float $y1): mixed` | `mixed` | ✓ | — |
| [`cairo_pattern_create_radial()`](./image/cairo_pattern_create_radial.md) | `(float $cx0, float $cy0, float $radius0, float $cx1, float $cy1, float $radius1): mixed` | `mixed` | ✓ | — |
| [`cairo_pattern_create_rgb()`](./image/cairo_pattern_create_rgb.md) | `(float $red, float $green, float $blue): mixed` | `mixed` | ✓ | — |
| [`cairo_pattern_create_rgba()`](./image/cairo_pattern_create_rgba.md) | `(float $red, float $green, float $blue, float $alpha): mixed` | `mixed` | ✓ | — |
| [`cairo_rectangle()`](./image/cairo_rectangle.md) | `(mixed $context, float $x, float $y, float $width, float $height): void` | `void` | ✓ | — |
| [`cairo_restore()`](./image/cairo_restore.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_rotate()`](./image/cairo_rotate.md) | `(mixed $context, float $angle): void` | `void` | ✓ | — |
| [`cairo_save()`](./image/cairo_save.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_scale()`](./image/cairo_scale.md) | `(mixed $context, float $sx, float $sy): void` | `void` | ✓ | — |
| [`cairo_set_fill_rule()`](./image/cairo_set_fill_rule.md) | `(mixed $context, int $fillRule): void` | `void` | ✓ | — |
| [`cairo_set_line_cap()`](./image/cairo_set_line_cap.md) | `(mixed $context, int $lineCap): void` | `void` | ✓ | — |
| [`cairo_set_line_join()`](./image/cairo_set_line_join.md) | `(mixed $context, int $lineJoin): void` | `void` | ✓ | — |
| [`cairo_set_line_width()`](./image/cairo_set_line_width.md) | `(mixed $context, float $width): void` | `void` | ✓ | — |
| [`cairo_set_matrix()`](./image/cairo_set_matrix.md) | `(mixed $context, mixed $matrix): void` | `void` | ✓ | — |
| [`cairo_set_source()`](./image/cairo_set_source.md) | `(mixed $context, mixed $pattern): void` | `void` | ✓ | — |
| [`cairo_set_source_rgb()`](./image/cairo_set_source_rgb.md) | `(mixed $context, float $red, float $green, float $blue): void` | `void` | ✓ | — |
| [`cairo_set_source_rgba()`](./image/cairo_set_source_rgba.md) | `(mixed $context, float $red, float $green, float $blue, float $alpha): void` | `void` | ✓ | — |
| [`cairo_stroke()`](./image/cairo_stroke.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_stroke_preserve()`](./image/cairo_stroke_preserve.md) | `(mixed $context): void` | `void` | ✓ | — |
| [`cairo_surface_write_to_png()`](./image/cairo_surface_write_to_png.md) | `(mixed $surface, string $filename): void` | `void` | ✓ | — |
| [`cairo_transform()`](./image/cairo_transform.md) | `(mixed $context, mixed $matrix): void` | `void` | ✓ | — |
| [`cairo_translate()`](./image/cairo_translate.md) | `(mixed $context, float $tx, float $ty): void` | `void` | ✓ | — |
| [`exif_imagetype()`](./image/exif_imagetype.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`exif_read_data()`](./image/exif_read_data.md) | `(string $filename, string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false): mixed` | `mixed` | ✓ | — |
| [`exif_tagname()`](./image/exif_tagname.md) | `(int $index): mixed` | `mixed` | ✓ | — |
| [`exif_thumbnail()`](./image/exif_thumbnail.md) | `(string $filename, mixed $width = 0, mixed $height = 0, mixed $image_type = 0): mixed` | `mixed` | ✓ | — |
| [`gd_info()`](./image/gd_info.md) | `(): mixed` | `mixed` | ✓ | — |
| [`getimagesize()`](./image/getimagesize.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`getimagesizefromstring()`](./image/getimagesizefromstring.md) | `(string $data): mixed` | `mixed` | ✓ | — |
| [`image_type_to_extension()`](./image/image_type_to_extension.md) | `(int $image_type, bool $include_dot = true): mixed` | `mixed` | ✓ | — |
| [`image_type_to_mime_type()`](./image/image_type_to_mime_type.md) | `(int $image_type): string` | `string` | ✓ | — |
| [`imageaffine()`](./image/imageaffine.md) | `(mixed $image, mixed $affine, mixed $clip = null): mixed` | `mixed` | ✓ | — |
| [`imageaffinematrixconcat()`](./image/imageaffinematrixconcat.md) | `(mixed $matrix1, mixed $matrix2): mixed` | `mixed` | ✓ | — |
| [`imagealphablending()`](./image/imagealphablending.md) | `(mixed $image, bool $enable): bool` | `bool` | ✓ | — |
| [`imageantialias()`](./image/imageantialias.md) | `(mixed $image, bool $enable): bool` | `bool` | ✓ | — |
| [`imagearc()`](./image/imagearc.md) | `(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color): bool` | `bool` | ✓ | — |
| [`imagebmp()`](./image/imagebmp.md) | `(mixed $image, string $file = null, bool $compressed = true): bool` | `bool` | ✓ | — |
| [`imagechar()`](./image/imagechar.md) | `(mixed $image, int $font, int $x, int $y, string $char, int $color): bool` | `bool` | ✓ | — |
| [`imagecharup()`](./image/imagecharup.md) | `(mixed $image, int $font, int $x, int $y, string $char, int $color): bool` | `bool` | ✓ | — |
| [`imagecolorallocate()`](./image/imagecolorallocate.md) | `(mixed $image, int $red, int $green, int $blue): int` | `int` | ✓ | — |
| [`imagecolorallocatealpha()`](./image/imagecolorallocatealpha.md) | `(mixed $image, int $red, int $green, int $blue, int $alpha): int` | `int` | ✓ | — |
| [`imagecolorat()`](./image/imagecolorat.md) | `(mixed $image, int $x, int $y): int` | `int` | ✓ | — |
| [`imagecolorclosest()`](./image/imagecolorclosest.md) | `(mixed $image, int $red, int $green, int $blue): int` | `int` | ✓ | — |
| [`imagecolorclosestalpha()`](./image/imagecolorclosestalpha.md) | `(mixed $image, int $red, int $green, int $blue, int $alpha): int` | `int` | ✓ | — |
| [`imagecolorclosesthwb()`](./image/imagecolorclosesthwb.md) | `(mixed $image, int $red, int $green, int $blue): int` | `int` | ✓ | — |
| [`imagecolordeallocate()`](./image/imagecolordeallocate.md) | `(mixed $image, int $color): bool` | `bool` | ✓ | — |
| [`imagecolorexact()`](./image/imagecolorexact.md) | `(mixed $image, int $red, int $green, int $blue): int` | `int` | ✓ | — |
| [`imagecolorexactalpha()`](./image/imagecolorexactalpha.md) | `(mixed $image, int $red, int $green, int $blue, int $alpha): int` | `int` | ✓ | — |
| [`imagecolormatch()`](./image/imagecolormatch.md) | `(mixed $image1, mixed $image2): bool` | `bool` | ✓ | — |
| [`imagecolorresolve()`](./image/imagecolorresolve.md) | `(mixed $image, int $red, int $green, int $blue): int` | `int` | ✓ | — |
| [`imagecolorresolvealpha()`](./image/imagecolorresolvealpha.md) | `(mixed $image, int $red, int $green, int $blue, int $alpha): int` | `int` | ✓ | — |
| [`imagecolorset()`](./image/imagecolorset.md) | `(mixed $image, int $color, int $red, int $green, int $blue, int $alpha = 0): bool` | `bool` | ✓ | — |
| [`imagecolorsforindex()`](./image/imagecolorsforindex.md) | `(mixed $image, int $color): mixed` | `mixed` | ✓ | — |
| [`imagecolorstotal()`](./image/imagecolorstotal.md) | `(mixed $image): int` | `int` | ✓ | — |
| [`imagecolortransparent()`](./image/imagecolortransparent.md) | `(mixed $image, int $color = null): int` | `int` | ✓ | — |
| [`imageconvolution()`](./image/imageconvolution.md) | `(mixed $image, mixed $matrix, float $divisor, float $offset): bool` | `bool` | ✓ | — |
| [`imagecopy()`](./image/imagecopy.md) | `(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height): bool` | `bool` | ✓ | — |
| [`imagecopymerge()`](./image/imagecopymerge.md) | `(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool` | `bool` | ✓ | — |
| [`imagecopymergegray()`](./image/imagecopymergegray.md) | `(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool` | `bool` | ✓ | — |
| [`imagecopyresampled()`](./image/imagecopyresampled.md) | `(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool` | `bool` | ✓ | — |
| [`imagecopyresized()`](./image/imagecopyresized.md) | `(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool` | `bool` | ✓ | — |
| [`imagecreate()`](./image/imagecreate.md) | `(int $width, int $height): mixed` | `mixed` | ✓ | — |
| [`imagecreatefrombmp()`](./image/imagecreatefrombmp.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`imagecreatefromgif()`](./image/imagecreatefromgif.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`imagecreatefromjpeg()`](./image/imagecreatefromjpeg.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`imagecreatefrompng()`](./image/imagecreatefrompng.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`imagecreatefromstring()`](./image/imagecreatefromstring.md) | `(string $data): mixed` | `mixed` | ✓ | — |
| [`imagecreatefromtga()`](./image/imagecreatefromtga.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`imagecreatefromwebp()`](./image/imagecreatefromwebp.md) | `(string $filename): mixed` | `mixed` | ✓ | — |
| [`imagecreatetruecolor()`](./image/imagecreatetruecolor.md) | `(int $width, int $height): mixed` | `mixed` | ✓ | — |
| [`imagecrop()`](./image/imagecrop.md) | `(mixed $image, mixed $rect = ['x' => 0, 'y' => 0, 'width' => 0, 'height' => 0]): mixed` | `mixed` | ✓ | — |
| [`imagecropauto()`](./image/imagecropauto.md) | `(mixed $image, int $mode = IMG_CROP_DEFAULT, float $threshold = 0.5, int $color = -1): mixed` | `mixed` | ✓ | — |
| [`imagedashedline()`](./image/imagedashedline.md) | `(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool` | `bool` | ✓ | — |
| [`imagedestroy()`](./image/imagedestroy.md) | `(mixed $image): bool` | `bool` | ✓ | — |
| [`imageellipse()`](./image/imageellipse.md) | `(mixed $image, int $center_x, int $center_y, int $width, int $height, int $color): bool` | `bool` | ✓ | — |
| [`imagefill()`](./image/imagefill.md) | `(mixed $image, int $x, int $y, int $color): bool` | `bool` | ✓ | — |
| [`imagefilledarc()`](./image/imagefilledarc.md) | `(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color, int $style): bool` | `bool` | ✓ | — |
| [`imagefilledellipse()`](./image/imagefilledellipse.md) | `(mixed $image, int $center_x, int $center_y, int $width, int $height, int $color): bool` | `bool` | ✓ | — |
| [`imagefilledpolygon()`](./image/imagefilledpolygon.md) | `(mixed $image, mixed $points, int $color): bool` | `bool` | ✓ | — |
| [`imagefilledrectangle()`](./image/imagefilledrectangle.md) | `(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool` | `bool` | ✓ | — |
| [`imagefilltoborder()`](./image/imagefilltoborder.md) | `(mixed $image, int $x, int $y, int $border_color, int $color): bool` | `bool` | ✓ | — |
| [`imagefilter()`](./image/imagefilter.md) | `(mixed $image, int $filter, int $arg1 = 0, int $arg2 = 0, int $arg3 = 0, int $arg4 = 0): bool` | `bool` | ✓ | — |
| [`imageflip()`](./image/imageflip.md) | `(mixed $image, int $mode): bool` | `bool` | ✓ | — |
| [`imagefontheight()`](./image/imagefontheight.md) | `(int $font): int` | `int` | ✓ | — |
| [`imagefontwidth()`](./image/imagefontwidth.md) | `(int $font): int` | `int` | ✓ | — |
| [`imagegammacorrect()`](./image/imagegammacorrect.md) | `(mixed $image, float $input_gamma, float $output_gamma): bool` | `bool` | ✓ | — |
| [`imagegetinterpolation()`](./image/imagegetinterpolation.md) | `(mixed $image): int` | `int` | ✓ | — |
| [`imagegif()`](./image/imagegif.md) | `(mixed $image, string $file = null): bool` | `bool` | ✓ | — |
| [`imageinterlace()`](./image/imageinterlace.md) | `(mixed $image, bool $enable = null): int` | `int` | ✓ | — |
| [`imageistruecolor()`](./image/imageistruecolor.md) | `(mixed $image): bool` | `bool` | ✓ | — |
| [`imagejpeg()`](./image/imagejpeg.md) | `(mixed $image, string $file = null, int $quality = -1): bool` | `bool` | ✓ | — |
| [`imagelayereffect()`](./image/imagelayereffect.md) | `(mixed $image, int $effect): bool` | `bool` | ✓ | — |
| [`imageline()`](./image/imageline.md) | `(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool` | `bool` | ✓ | — |
| [`imageopenpolygon()`](./image/imageopenpolygon.md) | `(mixed $image, mixed $points, int $color): bool` | `bool` | ✓ | — |
| [`imagepalettecopy()`](./image/imagepalettecopy.md) | `(mixed $dst, mixed $src): bool` | `bool` | ✓ | — |
| [`imagepalettetotruecolor()`](./image/imagepalettetotruecolor.md) | `(mixed $image): bool` | `bool` | ✓ | — |
| [`imagepng()`](./image/imagepng.md) | `(mixed $image, string $file = null, int $quality = -1, int $filters = -1): bool` | `bool` | ✓ | — |
| [`imagepolygon()`](./image/imagepolygon.md) | `(mixed $image, mixed $points, int $color): bool` | `bool` | ✓ | — |
| [`imagerectangle()`](./image/imagerectangle.md) | `(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool` | `bool` | ✓ | — |
| [`imageresolution()`](./image/imageresolution.md) | `(mixed $image, int $resolution_x = null, int $resolution_y = null): mixed` | `mixed` | ✓ | — |
| [`imagerotate()`](./image/imagerotate.md) | `(mixed $image, float $angle, int $background_color, int $ignore_transparent = 0): mixed` | `mixed` | ✓ | — |
| [`imagesavealpha()`](./image/imagesavealpha.md) | `(mixed $image, bool $enable): bool` | `bool` | ✓ | — |
| [`imagescale()`](./image/imagescale.md) | `(mixed $image, int $width, int $height = -1, int $mode = IMG_BILINEAR_FIXED): mixed` | `mixed` | ✓ | — |
| [`imagesetinterpolation()`](./image/imagesetinterpolation.md) | `(mixed $image, int $method = IMG_BILINEAR_FIXED): bool` | `bool` | ✓ | — |
| [`imagesetpixel()`](./image/imagesetpixel.md) | `(mixed $image, int $x, int $y, int $color): bool` | `bool` | ✓ | — |
| [`imagesetthickness()`](./image/imagesetthickness.md) | `(mixed $image, int $thickness): bool` | `bool` | ✓ | — |
| [`imagestring()`](./image/imagestring.md) | `(mixed $image, int $font, int $x, int $y, string $string, int $color): bool` | `bool` | ✓ | — |
| [`imagestringup()`](./image/imagestringup.md) | `(mixed $image, int $font, int $x, int $y, string $string, int $color): bool` | `bool` | ✓ | — |
| [`imagesx()`](./image/imagesx.md) | `(mixed $image): int` | `int` | ✓ | — |
| [`imagesy()`](./image/imagesy.md) | `(mixed $image): int` | `int` | ✓ | — |
| [`imagetruecolortopalette()`](./image/imagetruecolortopalette.md) | `(mixed $image, bool $dither, int $num_colors): bool` | `bool` | ✓ | — |
| [`imagetypes()`](./image/imagetypes.md) | `(): int` | `int` | ✓ | — |
| [`imagewebp()`](./image/imagewebp.md) | `(mixed $image, string $file = null, int $quality = -1): bool` | `bool` | ✓ | — |
| [`iptcembed()`](./image/iptcembed.md) | `(string $iptcdata, string $jpeg_file_name, int $spool = 0): mixed` | `mixed` | ✓ | — |
| [`iptcparse()`](./image/iptcparse.md) | `(string $iptcblock): mixed` | `mixed` | ✓ | — |
| [`read_exif_data()`](./image/read_exif_data.md) | `(string $filename, string $required_sections = null, bool $as_arrays = false, bool $read_thumbnail = false): mixed` | `mixed` | ✓ | — |
