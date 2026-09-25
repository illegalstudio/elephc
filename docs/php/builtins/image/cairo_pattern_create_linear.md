---
title: "cairo_pattern_create_linear()"
description: "Creates a linear gradient pattern between two points."
sidebar:
  order: 429
---

## cairo_pattern_create_linear()

```php
function cairo_pattern_create_linear(float $x0, float $y0, float $x1, float $y1): mixed
```

Creates a linear gradient pattern between two points.

**Parameters**:
- `$x0` (`float`)
- `$y0` (`float`)
- `$x1` (`float`)
- `$y1` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_pattern_create_linear` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_pattern_create_linear.md).
