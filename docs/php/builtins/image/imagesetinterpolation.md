---
title: "imagesetinterpolation()"
description: "Selects the interpolation method used when resampling."
sidebar:
  order: 534
---

## imagesetinterpolation()

```php
function imagesetinterpolation(mixed $image, int $method = IMG_BILINEAR_FIXED): bool
```

Selects the interpolation method used when resampling.

**Parameters**:
- `$image` (`mixed`)
- `$method` (`int`), default `IMG_BILINEAR_FIXED`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesetinterpolation` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesetinterpolation.md).
