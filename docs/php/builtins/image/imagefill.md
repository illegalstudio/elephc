---
title: "imagefill()"
description: "Flood-fills from a point with a color."
---

## imagefill()

```php
function imagefill(mixed $image, int $x, int $y, int $color): bool
```

Flood-fills from a point with a color.

**Parameters**:
- `$image` (`mixed`)
- `$x` (`int`)
- `$y` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefill` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefill.md).
