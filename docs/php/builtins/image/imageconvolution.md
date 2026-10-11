---
title: "imageconvolution()"
description: "Applies a 3x3 convolution matrix to an image."
sidebar:
  order: 484
---

## imageconvolution()

```php
function imageconvolution(mixed $image, array $matrix, float $divisor, float $offset): bool
```

Applies a 3x3 convolution matrix to an image.

**Parameters**:
- `$image` (`mixed`)
- `$matrix` (`array`)
- `$divisor` (`float`)
- `$offset` (`float`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageconvolution` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageconvolution.md).
