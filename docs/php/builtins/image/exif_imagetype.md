---
title: "exif_imagetype()"
description: "Identifies an image file's type, or false when it is not an image."
sidebar:
  order: 451
---

## exif_imagetype()

```php
function exif_imagetype(string $filename): mixed
```

Identifies an image file's type, or false when it is not an image.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exif_imagetype` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/exif_imagetype.md).
