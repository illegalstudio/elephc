---
title: "imagecolorexactalpha()"
description: "Returns the palette index of an exact color with alpha, or -1."
---

## imagecolorexactalpha()

```php
function imagecolorexactalpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Returns the palette index of an exact color with alpha, or -1.

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

For how `imagecolorexactalpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorexactalpha.md).
