---
title: "cairo_arc_negative()"
description: "Adds a counter-clockwise arc of the given radius and angle span to the current path."
sidebar:
  order: 404
---

## cairo_arc_negative()

```php
function cairo_arc_negative(mixed $context, float $xc, float $yc, float $radius, float $angle1, float $angle2): void
```

Adds a counter-clockwise arc of the given radius and angle span to the current path.

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

For how `cairo_arc_negative` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_arc_negative.md).
