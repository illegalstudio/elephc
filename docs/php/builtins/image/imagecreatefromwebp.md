---
title: "imagecreatefromwebp()"
description: "Creates an image from a WebP file."
sidebar:
  order: 495
---

## imagecreatefromwebp()

```php
function imagecreatefromwebp(string $filename): mixed
```

Creates an image from a WebP file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatefromwebp` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatefromwebp.md).
