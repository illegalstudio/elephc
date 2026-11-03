---
title: "cairo_rectangle()"
description: "Adds a closed rectangular subpath to the current path."
---

## cairo_rectangle()

```php
function cairo_rectangle(mixed $context, float $x, float $y, float $width, float $height): void
```

Adds a closed rectangular subpath to the current path.

**Parameters**:
- `$context` (`mixed`)
- `$x` (`float`)
- `$y` (`float`)
- `$width` (`float`)
- `$height` (`float`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_rectangle` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_rectangle.md).
