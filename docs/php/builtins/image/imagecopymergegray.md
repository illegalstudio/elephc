---
title: "imagecopymergegray()"
description: "Copies a rectangle into another image as grayscale, blending it by a percentage."
sidebar:
  order: 485
---

## imagecopymergegray()

```php
function imagecopymergegray(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height, int $pct): bool
```

Copies a rectangle into another image as grayscale, blending it by a percentage.

**Parameters**:
- `$dst_image` (`mixed`)
- `$src_image` (`mixed`)
- `$dst_x` (`int`)
- `$dst_y` (`int`)
- `$src_x` (`int`)
- `$src_y` (`int`)
- `$src_width` (`int`)
- `$src_height` (`int`)
- `$pct` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecopymergegray` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecopymergegray.md).
