---
title: "cairo_curve_to()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 402
---

## cairo_curve_to()

```php
function cairo_curve_to(mixed $context, float $x1, float $y1, float $x2, float $y2, float $x3, float $y3): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$x1` (`float`)
- `$y1` (`float`)
- `$x2` (`float`)
- `$y2` (`float`)
- `$x3` (`float`)
- `$y3` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_curve_to` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_curve_to.md).
