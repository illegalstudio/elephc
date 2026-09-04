---
title: "imagescale()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 526
---

## imagescale()

```php
function imagescale(mixed $image, int $width, int $height = -1, int $mode = IMG_BILINEAR_FIXED): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$width` (`int`)
- `$height` (`int`), default `-1`, optional
- `$mode` (`int`), default `IMG_BILINEAR_FIXED`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagescale` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagescale.md).
