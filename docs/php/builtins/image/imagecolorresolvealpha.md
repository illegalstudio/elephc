---
title: "imagecolorresolvealpha()"
description: "Returns the palette index of a color with alpha, allocating or approximating it."
---

## imagecolorresolvealpha()

```php
function imagecolorresolvealpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Returns the palette index of a color with alpha, allocating or approximating it.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)
- `$alpha` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorresolvealpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorresolvealpha.md).
