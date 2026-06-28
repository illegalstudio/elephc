---
title: "Images"
description: "GD image creation and I/O, plus the always-available image info functions, backed by a pure-Rust codec bridge."
sidebar:
  order: 18
---

elephc implements PHP's image surface (GD, Exif/IPTC, and the Imagick/Gmagick/Cairo
OOP extensions) on top of a **pure-Rust** image bridge. There is no dependency on
the system GD, libpng, libjpeg, or ImageMagick: the codecs are compiled into the
program, so binaries stay standalone and behave identically across macOS arm64,
Linux arm64, and Linux x86_64.

The image library is only linked when a program references an image symbol, so
programs that do not use images are unaffected.

> The GD, Exif/IPTC, Imagick, Gmagick, and Cairo surfaces are all implemented.
> Documented gaps (operations with no pure-Rust path) are called out inline.

## Image information (always available)

These functions do not require any extension in PHP and are always available.

### `getimagesize`

```php
getimagesize(string $filename): array|false
```

Returns information about an image file without fully decoding it, or `false` if
the file cannot be read or is not a recognized image. The returned array contains:

- `0` — width in pixels
- `1` — height in pixels
- `2` — one of the `IMAGETYPE_*` constants
- `3` — a `width="…" height="…"` HTML attribute string
- `"bits"` — bit depth
- `"channels"` — channel count
- `"mime"` — the MIME type

```php
$info = getimagesize("photo.png");
if ($info !== false) {
    echo $info[0] . "x" . $info[1] . " " . $info["mime"];   // 800x600 image/png
}
```

Because the returned array is heterogeneous, cast an element before passing it to
a typed parameter: `image_type_to_extension((int) $info[2])`.

### `getimagesizefromstring`

```php
getimagesizefromstring(string $data): array|false
```

Like `getimagesize`, but the image bytes arrive in a PHP string instead of a
file path — the bytes are staged into the bridge buffer and probed without full
decoding. Returns the same array shape as `getimagesize`, or `false` for an empty
string or unrecognized bytes. The optional `&$image_info` APP-markers parameter
PHP accepts is omitted (use `exif_read_data()` for tags).

```php
$data = file_get_contents("photo.png");
$info = getimagesizefromstring($data);
if ($info !== false) {
    echo $info[0] . "x" . $info[1] . " " . $info["mime"];   // 800x600 image/png
}
```

### `image_type_to_mime_type`

```php
image_type_to_mime_type(int $image_type): string
```

Maps an `IMAGETYPE_*` constant to its MIME type, e.g.
`image_type_to_mime_type(IMAGETYPE_PNG)` returns `"image/png"`.

### `image_type_to_extension`

```php
image_type_to_extension(int $image_type, bool $include_dot = true): string|false
```

Returns the file extension for an image type, with a leading dot by default:
`image_type_to_extension(IMAGETYPE_JPEG)` returns `".jpeg"`, and
`image_type_to_extension(IMAGETYPE_JPEG, false)` returns `"jpeg"`.

> Incompatibility: PHP returns `false` for an unknown image type. elephc currently
> returns `""` (the empty string) instead, pending end-to-end support for
> scalar-or-`false` union return values.

## GD: creating and writing images

### Creating an image

```php
imagecreatetruecolor(int $width, int $height): GdImage
imagecreate(int $width, int $height): GdImage
```

`imagecreatetruecolor` returns a `GdImage` object backed by an opaque native
handle. The image is freed automatically when the `GdImage` goes out of scope, or
explicitly with `imagedestroy()`.

### Colors and pixels

```php
imagecolorallocate(GdImage $image, int $red, int $green, int $blue): int
imagecolorallocatealpha(GdImage $image, int $red, int $green, int $blue, int $alpha): int
imagesetpixel(GdImage $image, int $x, int $y, int $color): bool
imagecolorat(GdImage $image, int $x, int $y): int
imagecolorsforindex(GdImage $image, int $color): array
```

Colors are GD packed integers; `$alpha` follows GD's 7-bit convention
(`0` = opaque, `127` = transparent). `imagecolorat` returns the packed color of a
pixel, and `imagecolorsforindex` unpacks one into `["red", "green", "blue",
"alpha"]`.

The palette-oriented lookups behave like their true-color equivalents (there is
no indexed palette to search — every image is RGBA), all returning a packed
color:

```php
imagecolorexact / imagecolorclosest / imagecolorclosesthwb / imagecolorresolve
    (GdImage $image, int $red, int $green, int $blue): int
imagecolorexactalpha / imagecolorclosestalpha / imagecolorresolvealpha
    (GdImage $image, int $red, int $green, int $blue, int $alpha): int
imagecolordeallocate(GdImage $image, int $color): bool   // no-op for true-color
imagecolorstotal(GdImage $image): int                    // 0 for true-color
imagecolortransparent(GdImage $image, ?int $color = null): int
imagecolorset(GdImage $image, int $color, int $red, int $green, int $blue, int $alpha = 0): bool
imagepalettecopy(GdImage $dst, GdImage $src): bool
```

`imagecolorset` and `imagepalettecopy` are accepted as no-op successes: elephc
stores every image as true-color RGBA with no indexed palette, so there are no
palette slots to recolor or copy. Code that depends on a palette color actually
changing after `imagecolorset` should treat that as a documented limitation.

### Alpha blending and saving

```php
imagealphablending(GdImage $image, bool $enable): bool
imagesavealpha(GdImage $image, bool $enable): bool
imagelayereffect(GdImage $image, int $effect): bool
imagepalettetotruecolor(GdImage $image): bool
imagetruecolortopalette(GdImage $image, bool $dither, int $num_colors): bool
```

With blending on (the true-color default), `imagesetpixel` composites the new
color over the existing pixel; with it off, the pixel is overwritten and its
alpha preserved. `imagesavealpha(true)` keeps the alpha channel when encoding
(the default flattens to opaque). `imagelayereffect` maps onto the blending model
(`IMG_EFFECT_REPLACE` disables blending; the others enable it).
`imagepalettetotruecolor`/`imagetruecolortopalette` flip the true-color flag;
since every image is stored as RGBA, `imagetruecolortopalette` does not actually
requantize.

### Drawing primitives

```php
imagesetthickness(GdImage $image, int $thickness): bool
imageline(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
imagedashedline(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
imagerectangle(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
imagefilledrectangle(GdImage $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
imageellipse(GdImage $image, int $cx, int $cy, int $width, int $height, int $color): bool
imagefilledellipse(GdImage $image, int $cx, int $cy, int $width, int $height, int $color): bool
imagearc(GdImage $image, int $cx, int $cy, int $w, int $h, int $start, int $end, int $color): bool
imagefilledarc(GdImage $image, int $cx, int $cy, int $w, int $h, int $start, int $end, int $color, int $style): bool
imagepolygon(GdImage $image, array $points, int $color): bool
imageopenpolygon(GdImage $image, array $points, int $color): bool
imagefilledpolygon(GdImage $image, array $points, int $color): bool
imagefill(GdImage $image, int $x, int $y, int $color): bool
imagefilltoborder(GdImage $image, int $x, int $y, int $border_color, int $color): bool
```

Outlines honor the current `imagesetthickness` and the alpha-blending mode;
`imagefill`/`imagefilltoborder` set the color directly (no blending), like GD.
Polygon `$points` is a flat array `[x0, y0, x1, y1, …]`. Arc angles use GD's
convention (0° at 3 o'clock, increasing clockwise); `imagefilledarc` fills a pie
slice, and `IMG_ARC_NOFILL` draws the outline instead (the `IMG_ARC_EDGED` radius
edges are approximated by that outline). The styled/brushed/tiled drawing modes
(`imagesetbrush`/`imagesetstyle`/`imagesettile`), clipping, and antialiasing are
not yet implemented.

### Text (built-in fonts)

```php
imagestring(GdImage $image, int $font, int $x, int $y, string $string, int $color): bool
imagestringup(GdImage $image, int $font, int $x, int $y, string $string, int $color): bool
imagechar(GdImage $image, int $font, int $x, int $y, string $char, int $color): bool
imagecharup(GdImage $image, int $font, int $x, int $y, string $char, int $color): bool
imagefontwidth(int $font): int
imagefontheight(int $font): int
```

Built-in text uses the public-domain `font8x8` glyph set. Every font number (1–5)
renders with the same uniform **8×8** cell, so `imagefontwidth`/`imagefontheight`
return `8` and the font number does not change the size (native GD's per-font
5–9×8–15 cells are not reproduced). `imagestringup` rotates the text 90°
counter-clockwise; `imagechar`/`imagecharup` draw the first character of the
string.

> Not yet implemented: TrueType/FreeType text (`imagettftext`, `imagettfbbox`,
> `imagefttext`, `imageftbbox`) and the `.gdf` font loader (`imageloadfont`).

### Size and cleanup

```php
imagesx(GdImage $image): int
imagesy(GdImage $image): int
imagedestroy(GdImage $image): bool
```

### Image info

```php
imageistruecolor(GdImage $image): bool
imageresolution(GdImage $image, ?int $resolution_x = null, ?int $resolution_y = null): array|bool
imagetypes(): int
gd_info(): array
```

`imageistruecolor` reports whether the image was created with
`imagecreatetruecolor` (true) or `imagecreate` (palette, false).
`imageresolution` returns `[res_x, res_y]` (default `[96, 96]`) when called with
one argument, or sets the DPI and returns `true` when given a resolution.
`imagetypes` returns the bitmask of supported formats (`IMG_PNG | IMG_JPG |
IMG_GIF | IMG_WEBP | IMG_BMP`), and `gd_info` returns the per-format capability
array.

## GD: reading and writing files

### Loading images

```php
imagecreatefrompng(string $filename): GdImage
imagecreatefromjpeg(string $filename): GdImage
imagecreatefromgif(string $filename): GdImage
imagecreatefrombmp(string $filename): GdImage
imagecreatefromwebp(string $filename): GdImage
imagecreatefromtga(string $filename): GdImage
imagecreatefromstring(string $data): GdImage
```

`imagecreatefrom*` decode a file (enforcing the named format), and
`imagecreatefromstring` decodes an in-memory string with auto-detection.

> **Incompatibility — failure is reported by throwing, not `false`.** PHP returns
> `GdImage|false` from these functions. elephc cannot return a `GdImage|false`
> that stays usable: the result must be passed to a `GdImage`-typed function, and
> the type checker neither accepts a `GdImage|false` argument there nor narrows it
> after a `=== false` check. So elephc returns a plain `GdImage` and throws an
> `ImageException` on failure. The common path is unchanged —
> `$im = imagecreatefrompng($f); imagesx($im);` — and error handling uses
> try/catch:
>
> ```php
> try {
>     $im = imagecreatefrompng($path);
> } catch (ImageException $e) {
>     // handle a missing/corrupt/wrong-format file
> }
> ```

### Writing images

```php
imagepng(GdImage $image, ?string $file = null, int $quality = -1, int $filters = -1): bool
imagejpeg(GdImage $image, ?string $file = null, int $quality = -1): bool
imagegif(GdImage $image, ?string $file = null): bool
imagebmp(GdImage $image, ?string $file = null, bool $compressed = true): bool
imagewebp(GdImage $image, ?string $file = null, int $quality = -1): bool
```

With a `$file` path the image is encoded straight to disk. With `$file` omitted
(or `null`) the encoded bytes are written to standard output, matching GD. JPEG
honors `$quality` (0–100). PNG is lossless (its `$quality`/`$filters` are accepted
but do not change pixels), WebP is encoded lossless, and BMP is uncompressed.

### Supported formats

PNG, JPEG, GIF, BMP, and WebP are fully supported for both reading and writing,
through the bundled pure-Rust codecs. TGA is supported for reading via
`imagecreatefromtga()` (PHP's TGA read support, since PHP 7.4); there is no TGA
writer. The remaining GD formats have no pure-Rust path yet and are **not
supported**: WBMP, XBM, XPM, the native GD/GD2 formats, and AVIF. Calling their
`imagecreatefrom*`/output functions is not available; this is tracked in the
roadmap and reported by `gd_info()`.

## GD: transforming, copying, and filtering

### Copying regions

```php
imagecopy(GdImage $dst, GdImage $src, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height): bool
imagecopymerge(GdImage $dst, GdImage $src, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool
imagecopymergegray(GdImage $dst, GdImage $src, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool
imagecopyresized(GdImage $dst, GdImage $src, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool
imagecopyresampled(GdImage $dst, GdImage $src, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool
```

`imagecopy` blits a region, compositing over the destination when its alpha
blending is on. `imagecopymerge` linearly blends the source over the destination
at `$pct` percent opacity; `imagecopymergegray` first desaturates the destination
to preserve the source hue. `imagecopyresized` scales with nearest-neighbour
sampling and `imagecopyresampled` with bilinear sampling. Copying within a single
image (`$dst === $src`) is safe — the source region is read before the
destination is written.

### Scaling, cropping, flipping, rotating

```php
imagescale(GdImage $image, int $width, int $height = -1, int $mode = IMG_BILINEAR_FIXED): GdImage
imagecrop(GdImage $image, array $rectangle): GdImage
imagecropauto(GdImage $image, int $mode = IMG_CROP_DEFAULT, float $threshold = 0.5, int $color = -1): GdImage
imageflip(GdImage $image, int $mode): bool
imagerotate(GdImage $image, float $angle, int $background_color, int $ignore_transparent = 0): GdImage
imageaffine(GdImage $image, array $affine, ?array $clip = null): GdImage
imageaffinematrixconcat(array $matrix1, array $matrix2): array
```

`imagescale` resizes the whole image (a negative `$height` preserves the aspect
ratio; `$mode` = `IMG_NEAREST_NEIGHBOUR` for nearest, otherwise bilinear).
`imagecrop` takes an associative rectangle `["x" => , "y" => , "width" => ,
"height" => ]` and pads any area past the source edge with transparency.
`imagecropauto` trims a uniform border (`IMG_CROP_DEFAULT`/`IMG_CROP_SIDES` use
the top-left pixel, plus `IMG_CROP_TRANSPARENT`/`IMG_CROP_BLACK`/`IMG_CROP_WHITE`/
`IMG_CROP_THRESHOLD`). `imageflip` mirrors in place (`IMG_FLIP_HORIZONTAL`,
`IMG_FLIP_VERTICAL`, `IMG_FLIP_BOTH`). `imagerotate` rotates counter-clockwise by
`$angle` degrees onto an enlarged canvas, filling exposed area with
`$background_color`; right-angle multiples are exact and other angles sample
nearest-neighbour. `imageaffine` applies a 6-element matrix `[a, b, c, d, e, f]`
(mapping `(x, y)` to `(a·x + c·y + e, b·x + d·y + f)`) and returns the transformed
bounding box; `imageaffinematrixconcat` composes two such matrices.

> **Incompatibility — failure is reported by throwing, not `false`.** Like
> `imagecreatefrom*`, the functions that return a *new* image
> (`imagescale`, `imagecrop`, `imagecropauto`, `imagerotate`, `imageaffine`)
> return a plain `GdImage` and throw an `ImageException` on failure, because a
> `GdImage|false` result cannot be passed to a `GdImage`-typed function and is not
> narrowed after a `=== false` check. The `$clip` argument of `imageaffine` is
> accepted but ignored (the full transformed image is always returned), and
> `imageaffinematrixget` is not yet implemented (its `array|float` parameter has
> no single-type representation in elephc).

### Filters and color adjustment

```php
imagefilter(GdImage $image, int $filter, int $arg1 = 0, int $arg2 = 0, int $arg3 = 0, int $arg4 = 0): bool
imageconvolution(GdImage $image, array $matrix, float $divisor, float $offset): bool
imagegammacorrect(GdImage $image, float $input_gamma, float $output_gamma): bool
```

`imagefilter` supports every GD selector: `IMG_FILTER_NEGATE`,
`IMG_FILTER_GRAYSCALE`, `IMG_FILTER_BRIGHTNESS`, `IMG_FILTER_CONTRAST`,
`IMG_FILTER_COLORIZE`, `IMG_FILTER_EDGEDETECT`, `IMG_FILTER_EMBOSS`,
`IMG_FILTER_GAUSSIAN_BLUR`, `IMG_FILTER_SELECTIVE_BLUR`, `IMG_FILTER_MEAN_REMOVAL`,
`IMG_FILTER_SMOOTH`, `IMG_FILTER_PIXELATE`, and `IMG_FILTER_SCATTER`. The
convolution-based filters use libgd's 3×3 kernels; `IMG_FILTER_SELECTIVE_BLUR` is
approximated by the gaussian kernel, and `IMG_FILTER_SCATTER` uses a deterministic
pseudo-random sequence (GD's is non-reproducible). The colors-array form of
`IMG_FILTER_SCATTER` is not supported. `imageconvolution` applies an arbitrary 3×3
matrix with edge replication, preserving alpha. `imagegammacorrect` raises each
channel to `input_gamma / output_gamma`.

### Interpolation, interlace, antialias

```php
imagesetinterpolation(GdImage $image, int $method = IMG_BILINEAR_FIXED): bool
imagegetinterpolation(GdImage $image): int
imageinterlace(GdImage $image, ?bool $enable = null): int
imageantialias(GdImage $image, bool $enable): bool
```

`imagesetinterpolation`/`imagegetinterpolation` store and report the interpolation
method (the `IMG_BELL` … `IMG_WEIGHTED4` constants). `imageinterlace` stores and
returns the interlace bit (the bundled encoders always write non-interlaced
output). `imageantialias` is accepted as a no-op — antialiased primitive drawing
is not implemented.

> Deprecated/removed GD converters (`image2wbmp`, `jpeg2wbmp`, `png2wbmp`) and the
> Windows-only screen-capture functions (`imagegrabscreen`, `imagegrabwindow`) are
> not provided.

## Exif and IPTC metadata

Read EXIF (camera/TIFF) tags and IPTC (caption/keyword) records. EXIF parsing is
backed by a pure-Rust EXIF/TIFF reader; IPTC is parsed and embedded directly, so
no system libexif is required.

### Reading EXIF

```php
exif_imagetype(string $filename): int|false
exif_read_data(string $filename, ?string $sections = null, bool $as_arrays = false, bool $thumbnail = false): array|false
read_exif_data(string $filename, ...): array|false
exif_tagname(int $index): string
exif_thumbnail(string $filename, &$width = 0, &$height = 0, &$image_type = 0): string
```

`exif_imagetype` returns the `IMAGETYPE_*` code from a header sniff, or `false`
for a non-image/missing file. `exif_read_data` (and its alias `read_exif_data`)
returns a flat associative array keyed by the standard EXIF mnemonics (`Make`,
`Model`, `Orientation`, `DateTime`, `XResolution`, `GPSLatitude`, …), or `false`
when the file has no EXIF block. `exif_tagname` maps a tag number to its mnemonic.
`exif_thumbnail` returns the embedded JPEG thumbnail and fills the by-reference
`width`/`height`/`image_type` out-parameters.

```php
$exif = exif_read_data("photo.jpg");
echo $exif["Make"], " ", $exif["Model"], "\n";
echo exif_tagname(0x0112), "\n";                 // Orientation

$thumb = exif_thumbnail("photo.jpg", $w, $h, $type);
if ($thumb !== "") {
    file_put_contents("thumb.jpg", $thumb);      // $w x $h, $type = IMAGETYPE_JPEG
}
```

PHP-compatibility notes (documented simplifications):

- EXIF values are returned as **strings**: ASCII as text, integer types as
  decimals, rationals as `"num/den"`. PHP returns typed scalars and arrays. A tag
  present in more than one IFD keeps its last value.
- The synthetic `FILE` / `COMPUTED` / `SectionsFound` meta-sections PHP injects
  into `exif_read_data` are not produced; use `getimagesize()` / `exif_imagetype()`
  for file-level data. The `$sections`, `$as_arrays`, and `$thumbnail` arguments
  are accepted for signature compatibility and do not change the returned tags.
- `exif_tagname` (unknown tag) and `exif_thumbnail` (no thumbnail) return `""`
  rather than `false`, because elephc collapses a `string|false` return to string.
  Test those with `=== ""` instead of `=== false`.
- Only JPEG-compressed EXIF thumbnails are extracted; the rare uncompressed-TIFF
  thumbnail form is not.

### IPTC

```php
iptcparse(string $iptcblock): array|false
iptcembed(string $iptcdata, string $jpeg_file_name, int $spool = 0): string|false
```

`iptcparse` decodes a raw IPTC IIM block into an array keyed by
`record#dataset` (e.g. `"2#005"`, `"2#025"`), where each key holds an array of
values (a repeated dataset such as keywords yields multiple entries). It returns
`false` if the block has no IPTC marker. `iptcembed` inserts an IPTC block into a
JPEG as a Photoshop `APP13` marker (replacing any existing one) and returns the
new JPEG bytes; when `$spool >= 2` it also echoes them.

```php
$data = iptcparse($block);
echo $data["2#005"][0], "\n";                    // caption
echo implode(", ", $data["2#025"]), "\n";        // keywords

$tagged = iptcembed($block, "photo.jpg");
file_put_contents("tagged.jpg", $tagged);
```

## Imagick (object API)

elephc provides the **Imagick** object API — `Imagick`, `ImagickDraw`,
`ImagickPixel`, `ImagickPixelIterator`, and `ImagickKernel` — as a pure-Rust
reimplementation over the same codec bridge that backs GD. **No system
ImageMagick is required**; the produced binaries are standalone on every target.

Because there is no pure-Rust port of ImageMagick, this is a *semantic*
reimplementation: it reproduces documented behavior but is **not byte-identical**
to ImageMagick. The **entire PHP API surface is declared**: every method of
`Imagick`, `ImagickDraw`, `ImagickPixel`, `ImagickPixelIterator`, and
`ImagickKernel` is callable, so any ImageMagick program compiles. The methods
shown below have pure-Rust implementations; every other method is a throwing
stub that raises `ImagickException("<Class>::<method>() is not supported in
elephc")` at runtime rather than failing silently or producing a compile-time
"Undefined method" error (see *Operations that throw* below).

### Creating and reading

```php
$im = new Imagick();                 // empty wand
$im->newImage(200, 120, "white");    // background: name, #hex, rgb()/rgba(), or ImagickPixel
$im->setImageFormat("PNG");

$photo = new Imagick("photo.jpg");   // read a file via the constructor
$photo->readImage("other.png");      // or explicitly
$photo->readImageBlob($bytes);       // or from in-memory bytes
```

`getImageWidth()`, `getImageHeight()`, and `getImageGeometry()` report the
current frame's size. `getImagePixelColor($x, $y)` returns an `ImagickPixel`.

### Writing and blobs

```php
$im->setImageCompressionQuality(90);
$im->writeImage("out.png");          // format from the extension or setImageFormat()
$bytes = $im->getImageBlob();        // encoded bytes (binary-safe)
```

### Transforms and effects

```php
$im->resizeImage(400, 300, Imagick::FILTER_LANCZOS, 1.0); // bestfit optional 5th arg
$im->scaleImage(100, 75);
$im->thumbnailImage(120, 0);         // 0 in one dimension keeps the aspect ratio
$im->cropImage(80, 80, 10, 10);
$im->rotateImage("white", 90);       // background color, then degrees
$im->flipImage();                    // vertical
$im->flopImage();                    // horizontal
$im->blurImage(2, 1);                // radius, sigma (Gaussian)
$im->gaussianBlurImage(2, 1);
$im->sharpenImage(1, 0.5);
$im->negateImage();
$im->modulateImage(110, 120, 100);   // brightness, saturation, hue (100 = unchanged)
```

The resize **filter** and **blur** arguments are accepted for API compatibility;
elephc resizes with a bilinear filter regardless. Resampling/sampling is faithful
but not pixel-identical to ImageMagick.

### Compositing

```php
$canvas->compositeImage($overlay, Imagick::COMPOSITE_OVER, $x, $y);
$canvas->compositeImage($overlay, Imagick::COMPOSITE_COPY, $x, $y);
```

`COMPOSITE_OVER` (source-over alpha blend) and `COMPOSITE_COPY` (overwrite) are
implemented. Any other composite operator throws `ImagickException`.

### Drawing with ImagickDraw

```php
$draw = new ImagickDraw();
$draw->setFillColor("#1d4ed8");                 // string or ImagickPixel
$draw->setStrokeColor(new ImagickPixel("black"));
$draw->setStrokeWidth(2);
$draw->rectangle(10, 10, 190, 60);
$draw->line(0, 0, 199, 119);
$draw->circle(50, 90, 50, 70);                  // center, then a perimeter point
$draw->ellipse(100, 60, 40, 25, 0, 360);
$draw->point(5, 5);
$draw->polygon([["x" => 0, "y" => 0], ["x" => 20, "y" => 0], ["x" => 10, "y" => 20]]);

$im->drawImage($draw);
```

Filled shapes paint the fill color first, then the stroke outline.

### ImagickPixel

```php
$p = new ImagickPixel("rgb(255,128,0)");
$p->getColor();                       // ["r"=>255, "g"=>128, "b"=>0, "a"=>255]
$p->getColor(1);                      // normalized 0..1 floats
$p->getColorValue(Imagick::COLOR_RED);// one channel as a 0..1 float
$p->getColorAsString();              // "srgb(255,128,0)"
$p->isSimilar(new ImagickPixel("orange"), 0.1);
```

### Multi-frame wands

`Imagick` is a sequence of frames and implements **Iterator** and **Countable**:

```php
$seq = new Imagick();
$seq->addImage($frameA);
$seq->addImage($frameB);
echo count($seq);                     // 2 (Countable)
foreach ($seq as $i => $frame) {      // Iterator: $frame is the wand at frame $i
    echo $frame->getImageWidth();
}
$seq->setImageIndex(1);               // select the active frame
$seq->nextImage(); $seq->previousImage(); $seq->setFirstIterator();
```

A per-pixel iterator is available via `getPixelIterator()`:

```php
$it = $im->getPixelIterator();
foreach ($it as $row) {               // each $row is an array of ImagickPixel
    foreach ($row as $pixel) { /* ... */ }
}
```

### Convolution with ImagickKernel

```php
$kernel = ImagickKernel::fromMatrix([
    [0, -1, 0],
    [-1, 5, -1],
    [0, -1, 0],
]);
$im->convolveImage($kernel);          // 3x3 kernels supported
```

`ImagickKernel::fromMatrix` supports **3×3** kernels; other sizes and
`fromBuiltIn` throw `ImagickKernelException` (documented gap).

### Operations that throw

The methods shown above have pure-Rust implementations. **Every other method**
in the Imagick family — the full PHP `Imagick`/`ImagickDraw`/`ImagickPixel`/
`ImagickPixelIterator`/`ImagickKernel` surface — is declared as a throwing stub
that raises `ImagickException("<Class>::<method>() is not supported in elephc")`
when called. A program that uses any of them still compiles; the exotic
operation simply throws at runtime. Examples that throw: `distortImage`,
`liquidRescaleImage`, `fxImage`, `waveImage`, `swirlImage`, `quantizeImage`,
`sepiaToneImage`, `identifyImage`, and `annotateImage` (the last needs FreeType
text). Catch the exception to detect an unsupported operation:

```php
try {
    $im->distortImage(Imagick::DISTORTION_PERSPECTIVE ?? 0, [/* ... */], false);
} catch (ImagickException $e) {
    // fall back to a supported transform
}
```

Other differences from ImageMagick: colors are parsed from CSS names / hex /
`rgb()` (a common subset, not the full X11 list); `setImageBackgroundColor()`
paints the current frame (elephc has no deferred background slot); and image
output uses the pure-Rust codecs (PNG/JPEG/GIF/BMP/WebP), so encoders are not
byte-identical.

## Gmagick (object API)

`Gmagick`, `GmagickDraw`, and `GmagickPixel` provide the GraphicsMagick-style
object API on the same pure-Rust bridge as Imagick — no system GraphicsMagick is
required. Gmagick methods are **fluent**: mutating operations return the
`Gmagick` object so calls can be chained. As with Imagick, the **entire PHP API
surface is declared**: every method is callable, the ones below have pure-Rust
implementations, and the rest are throwing stubs (see *Operations that throw*).

### Creating, drawing, and writing

```php
$gm = new Gmagick();
$gm->newImage(120, 60, "rgb(245,245,245)", "PNG");

$draw = new GmagickDraw();
$draw->setFillColor("#1d4ed8")->rectangle(8, 8, 60, 50);
$gm->drawImage($draw);

$gm->setCompressionQuality(90)
   ->scaleImage(240, 120)
   ->writeImage("banner.png");

echo $gm->getImageWidth() . "x" . $gm->getImageHeight();   // 240x120
```

`GmagickDraw` supports `setFillColor`/`setStrokeColor` (string or `GmagickPixel`),
`setStrokeWidth`, `line`, `rectangle`, `ellipse`, `point`, and `polygon` (an array
of `["x" => , "y" => ]` points), each returning the draw object for chaining.

### Transforms, effects, and frames

`resizeImage`, `scaleImage`, `thumbnailImage`, `cropImage`, `rotateImage`
(background color + degrees), `flipImage`, `flopImage`, `blurImage`,
`gaussianBlurImage`, `modulateImage`, and `compositeImage` (OVER/COPY) all return
the `Gmagick` object. Multiple frames are managed with `addImage`,
`getNumberImages`, `getImageIndex`/`setImageIndex`, `nextImage`/`previousImage`,
and `hasNextImage`/`hasPreviousImage`.

### Reading pixels

Gmagick has no per-pixel read method (matching PHP). To inspect output, encode the
frame with `getImageBlob()` and decode it through GD:

```php
$img = imagecreatefromstring($gm->getImageBlob());
$rgb = imagecolorat($img, 9, 9);   // 0xRRGGBB
```

### GmagickPixel

```php
$p = new GmagickPixel("rgb(255,0,0)");
echo $p->getColorAsString();              // srgb(255,0,0)
$c = $p->getColor();                      // ["r"=>255, "g"=>0, "b"=>0, "a"=>255]
echo $p->getColorValue(Gmagick::COLOR_RED);  // 1
$p->setColorValue(Gmagick::COLOR_GREEN, 1.0);
```

Unlike GraphicsMagick (where `getColor()` can return a textual form), elephc's
`GmagickPixel::getColor()` always returns the channel array; the string form is
available through `getColorAsString()`.

### Operations that throw

The methods shown above have pure-Rust implementations. **Every other method**
in the Gmagick family — the full PHP `Gmagick`/`GmagickDraw`/`GmagickPixel`
surface — is declared as a throwing stub that raises
`GmagickException("<Class>::<method>() is not supported in elephc")` when
called, so any GraphicsMagick program compiles. Examples that throw:
`swirlImage`, `charcoalImage`, `oilPaintImage`, `embossImage`, `quantizeImage`,
and `annotateImage` (FreeType text). `GmagickDraw::annotate` likewise throws
`GmagickDrawException`. An unrecognized color throws `GmagickPixelException`,
and an unsupported composite operator throws `GmagickException`. As with
Imagick, `setImageBackgroundColor()` paints the current frame and the pure-Rust
codecs are not byte-identical to GraphicsMagick.

## Cairo (object API)

The `Cairo*` classes provide cairo-style 2D vector drawing backed by the pure-Rust
[`tiny-skia`](https://crates.io/crates/tiny-skia) rasterizer — no system cairo is
required. `CairoImageSurface` (raster, PNG output) is fully supported; PDF, PS, and
SVG surfaces have no pure-Rust path and are documented gaps.

### Drawing to an image surface

```php
$surface = new CairoImageSurface(CairoFormat::ARGB32, 160, 120);
$cr = new CairoContext($surface);

$cr->setSourceRgb(1, 1, 1);          // white
$cr->paint();                         // flood the surface

$cr->setSourceRgb(0.114, 0.306, 0.847);
$cr->arc(48, 60, 34, 0, 2 * M_PI);    // a circle
$cr->fill();

$cr->setSourceRgb(0, 0.5, 0);
$cr->setLineWidth(4);
$cr->moveTo(96, 90);
$cr->lineTo(120, 40);
$cr->lineTo(144, 90);
$cr->closePath();
$cr->stroke();

$surface->writeToPng("out.png");
```

`CairoContext` supports `moveTo`/`lineTo`/`curveTo`/`arc`/`arcNegative`/`rectangle`/
`closePath`/`newPath`/`newSubPath`, `fill`/`fillPreserve`/`stroke`/`strokePreserve`/
`paint`, `setSourceRgb`/`setSourceRgba`/`setSource`, `setLineWidth`/`setLineCap`/
`setLineJoin`/`setFillRule`, the transform stack (`save`/`restore`,
`translate`/`scale`/`rotate`/`transform`/`setMatrix`/`identityMatrix`), and
`getCurrentPoint`.

### Patterns and gradients

```php
$grad = new CairoLinearGradient(0, 0, 160, 0);
$grad->addColorStopRgb(0, 1, 0, 0);   // red at the left
$grad->addColorStopRgb(1, 0, 0, 1);   // blue at the right
$cr->setSource($grad);
$cr->rectangle(0, 0, 160, 24);
$cr->fill();
```

`CairoSolidPattern`, `CairoLinearGradient`, and `CairoRadialGradient` are supported.
`CairoMatrix` is a value object (`initIdentity`/`initTranslate`/`initScale`/
`initRotate`/`transformPoint`).

### Reading pixels back

`CairoImageSurface` writes PNG; decode it through GD to inspect pixels:

```php
$img = imagecreatefrompng("out.png");
$rgb = imagecolorat($img, 48, 60);    // 0xRRGGBB
```

### Documented gaps

`CairoPdfSurface`, `CairoPsSurface`, and `CairoSvgSurface` throw `CairoException`
(no pure-Rust vector-document backend). Text rendering is FreeType-dependent:
`CairoContext::showText`/`textExtents`, `CairoToyFontFace`, and `CairoScaledFont`
throw `CairoException` (`selectFontFace`/`setFontSize` are accepted no-ops so
non-text drawing is unaffected). `CairoSurfacePattern` is unsupported. Output uses
tiny-skia's rasterizer, so it is anti-aliased but not byte-identical to cairo.

### Coverage model (intentionally partial)

Unlike the Imagick and Gmagick families — where *every* PHP method is declared as
either a real implementation or a throwing stub so the whole API surface is
callable — the Cairo surface is intentionally **partial**. PECL cairo is
experimental and low-usage, so elephc keeps three tiers:

1. **Implemented** — the common drawing path (`CairoContext` path/render/source/
   line-attribute/transform methods, `CairoImageSurface` raster + PNG,
   `CairoSolidPattern`/`CairoLinearGradient`/`CairoRadialGradient`, `CairoMatrix`).
2. **Throwing stubs** — documented gaps that have no pure-Rust equivalent (vector
   surfaces, text, surface patterns) throw `CairoException` at runtime.
3. **Undefined** — the remaining exotic methods (`clip`, `clipPreserve`,
   `resetClip`, `setDash`, `setMiterLimit`, `hasCurrentPoint`, `inFill`,
   `copyPath`, …) are *not declared*, so a call is a compile-time
   `Undefined method` error rather than a silent no-op.

This is deliberate: a compile error is preferable to silently dropping clipping or
dashing. The contract is locked by
`test_image_error_cairo_undefined_methods_are_compile_errors`.

## Cairo (procedural API)

The PECL-style free-function layer mirrors the `Cairo*` classes for callers that
prefer functions over objects. Each wrapper builds the corresponding object and
delegates, so the two layers behave identically.

```php
$surface = cairo_image_surface_create(CairoFormat::ARGB32, 160, 120);
$cr = cairo_create($surface);

cairo_set_source_rgb($cr, 1, 1, 1);
cairo_paint($cr);

cairo_set_source_rgb($cr, 0.114, 0.306, 0.847);
cairo_arc($cr, 48, 60, 34, 0, 2 * M_PI);
cairo_fill($cr);

cairo_surface_write_to_png($surface, "out.png");
```

The supported common subset is:

- **Surfaces:** `cairo_image_surface_create`, `cairo_image_surface_create_from_png`,
  `cairo_image_surface_get_width`, `cairo_image_surface_get_height`,
  `cairo_surface_write_to_png`.
- **Context:** `cairo_create`, `cairo_save`, `cairo_restore`.
- **Source/color:** `cairo_set_source_rgb`, `cairo_set_source_rgba`, `cairo_set_source`,
  `cairo_set_line_width`, `cairo_set_line_cap`, `cairo_set_line_join`,
  `cairo_set_fill_rule`.
- **Paths:** `cairo_move_to`, `cairo_line_to`, `cairo_curve_to`, `cairo_rectangle`,
  `cairo_arc`, `cairo_arc_negative`, `cairo_close_path`, `cairo_new_path`,
  `cairo_new_sub_path`.
- **Render:** `cairo_paint`, `cairo_fill`, `cairo_fill_preserve`, `cairo_stroke`,
  `cairo_stroke_preserve`.
- **Transform:** `cairo_translate`, `cairo_scale`, `cairo_rotate`, `cairo_set_matrix`,
  `cairo_transform`, `cairo_identity_matrix`, `cairo_get_current_point`.
- **Patterns:** `cairo_pattern_create_rgba`, `cairo_pattern_create_rgb`,
  `cairo_pattern_create_linear`, `cairo_pattern_create_radial`,
  `cairo_pattern_add_color_stop_rgb`, `cairo_pattern_add_color_stop_rgba`.
- **Matrices:** `cairo_matrix_init_identity`, `cairo_matrix_init_translate`,
  `cairo_matrix_init_scale`, `cairo_matrix_init_rotate`, `cairo_matrix_multiply`,
  `cairo_matrix_transform_point`.

`cairo_get_current_point` and `cairo_matrix_transform_point` return an
`["x" => …, "y" => …]` associative array. `cairo_matrix_multiply($m1, $m2)`
composes two matrices with `$m2` applied first, then `$m1` (cairo convention).

The obscure PECL tail (font options, scaled fonts, toy font faces, PDF/PS/SVG
surface constructors, and the remaining ~40 rarely-used helpers) is **not**
provided. Omitted functions are genuinely undefined — a call to one is a
compile-time `Undefined function:` error, not a silent stub. The object API
above is the supported way to reach the documented-gap behavior (vector
surfaces and text throw `CairoException`).

## Example

```php
<?php
$im = imagecreatetruecolor(64, 48);
$red = imagecolorallocate($im, 220, 40, 40);
imagesetpixel($im, 0, 0, $red);

imagepng($im, "out.png");
imagejpeg($im, "out.jpg", 90);

$info = getimagesize("out.png");
echo $info[0] . "x" . $info[1] . " " . $info["mime"] . "\n";   // 64x48 image/png

// Decode straight from bytes.
$copy = imagecreatefromstring((string) file_get_contents("out.png"));
echo imagesx($copy) . "x" . imagesy($copy) . "\n";             // 64x48

imagedestroy($copy);
imagedestroy($im);
```
