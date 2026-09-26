---
title: "exif_thumbnail()"
description: "Returns the embedded thumbnail of an image, with its size and type."
sidebar:
  order: 452
---

## exif_thumbnail()

```php
function exif_thumbnail(string $filename, mixed $width = 0, mixed $height = 0, mixed $image_type = 0): mixed
```

Returns the embedded thumbnail of an image, with its size and type.

**Parameters**:
- `$filename` (`string`)
- `$width` (`mixed`), passed by reference, default `0`, optional
- `$height` (`mixed`), passed by reference, default `0`, optional
- `$image_type` (`mixed`), passed by reference, default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exif_thumbnail` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/exif_thumbnail.md).
