---
title: "imagewebp()"
description: "Writes an image as WebP, to a file or to the output buffer."
sidebar:
  order: 539
---

## imagewebp()

```php
function imagewebp(mixed $image, ?string $file = null, int $quality = -1): bool
```

Writes an image as WebP, to a file or to the output buffer.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`?string`), default `null`, optional
- `$quality` (`int`), default `-1`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagewebp` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagewebp.md).
