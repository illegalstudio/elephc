---
title: "cairo_arc()"
description: "Adds a clockwise arc of the given radius and angle span to the current path."
sidebar:
  order: 401
---

## cairo_arc()

```php
function cairo_arc(mixed $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void
```

Adds a clockwise arc of the given radius and angle span to the current path.

**Parameters**:
- `$context` (`mixed`)
- `$xc` (`float`)
- `$yc` (`float`)
- `$radius` (`float`)
- `$angle1` (`float`)
- `$angle2` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_arc` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_arc.md).
