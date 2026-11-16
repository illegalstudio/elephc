---
title: "imagecolorat()"
description: "Returns the color index or packed color of one pixel."
---

## imagecolorat()

```php
function imagecolorat(mixed $image, int $x, int $y): int
```

Returns the color index or packed color of one pixel.

**Parameters**:
- `$image` (`mixed`)
- `$x` (`int`)
- `$y` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorat` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorat.md).
