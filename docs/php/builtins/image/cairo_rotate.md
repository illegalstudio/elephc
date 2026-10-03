---
title: "cairo_rotate()"
description: "Rotates the context's transformation by the given angle in radians."
sidebar:
  order: 435
---

## cairo_rotate()

```php
function cairo_rotate(mixed $context, float $angle): void
```

Rotates the context's transformation by the given angle in radians.

**Parameters**:
- `$context` (`mixed`)
- `$angle` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_rotate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_rotate.md).
