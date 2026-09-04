---
title: "imageconvolution()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 479
---

## imageconvolution()

```php
function imageconvolution(mixed $image, mixed $matrix, float $divisor, float $offset): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$matrix` (`mixed`)
- `$divisor` (`float`)
- `$offset` (`float`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageconvolution` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageconvolution.md).
