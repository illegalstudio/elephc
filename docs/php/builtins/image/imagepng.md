---
title: "imagepng()"
description: "Writes an image as PNG, to a file or to the output buffer."
sidebar:
  order: 523
---

## imagepng()

```php
function imagepng(mixed $image, ?string $file = null, int $quality = -1, int $filters = -1): bool
```

Writes an image as PNG, to a file or to the output buffer.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`?string`), default `null`, optional
- `$quality` (`int`), default `-1`, optional
- `$filters` (`int`), default `-1`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagepng` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagepng.md).
