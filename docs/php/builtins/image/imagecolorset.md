---
title: "imagecolorset()"
description: "Changes the color a palette index stands for."
---

## imagecolorset()

```php
function imagecolorset(mixed $image, int $color, int $red, int $green, int $blue, int $alpha = 0): bool
```

Changes the color a palette index stands for.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`int`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)
- `$alpha` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorset` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorset.md).
