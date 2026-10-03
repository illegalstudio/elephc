---
title: "cairo_line_to()"
description: "Adds a straight line from the current point to the given point."
sidebar:
  order: 416
---

## cairo_line_to()

```php
function cairo_line_to(mixed $context, float $x, float $y): void
```

Adds a straight line from the current point to the given point.

**Parameters**:
- `$context` (`mixed`)
- `$x` (`float`)
- `$y` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_line_to` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_line_to.md).
