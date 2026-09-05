---
title: "imagecopy()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 480
---

## imagecopy()

```php
function imagecopy(mixed $dst_image, mixed $src_image, int $dst_x, int $dst_y, int $src_x, int $src_y, int $src_width, int $src_height): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$dst_image` (`mixed`)
- `$src_image` (`mixed`)
- `$dst_x` (`int`)
- `$dst_y` (`int`)
- `$src_x` (`int`)
- `$src_y` (`int`)
- `$src_width` (`int`)
- `$src_height` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecopy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecopy.md).
