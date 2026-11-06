---
title: "cairo_matrix_transform_point()"
description: "Applies a matrix to a point and returns the transformed [x, y] pair."
---

## cairo_matrix_transform_point()

```php
function cairo_matrix_transform_point(mixed $matrix, float $x, float $y): array
```

Applies a matrix to a point and returns the transformed [x, y] pair.

**Parameters**:
- `$matrix` (`mixed`)
- `$x` (`float`)
- `$y` (`float`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_matrix_transform_point` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_matrix_transform_point.md).
