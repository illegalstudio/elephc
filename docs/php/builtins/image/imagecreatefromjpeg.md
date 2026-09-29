---
title: "imagecreatefromjpeg()"
description: "Creates an image from a JPEG file."
sidebar:
  order: 491
---

## imagecreatefromjpeg()

```php
function imagecreatefromjpeg(string $filename): mixed
```

Creates an image from a JPEG file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromjpeg` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromjpeg.md).
