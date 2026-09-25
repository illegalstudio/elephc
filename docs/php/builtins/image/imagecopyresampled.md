---
title: "imagecopyresampled()"
description: "Copies and resizes a rectangle with pixel interpolation."
sidebar:
  order: 488
---

## imagecopyresampled()

```php
function imagecopyresampled(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $dst_width, int $dst_height, int $src_width, int $src_height): bool
```

Copies and resizes a rectangle with pixel interpolation.

**Parameters**:
- `$dst_image` (`mixed`)
- `$src_image` (`mixed`)
- `$dst_x` (`int`)
- `$dst_y` (`int`)
- `$src_x` (`int`)
- `$src_y` (`int`)
- `$dst_width` (`int`)
- `$dst_height` (`int`)
- `$src_width` (`int`)
- `$src_height` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecopyresampled` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecopyresampled.md).
