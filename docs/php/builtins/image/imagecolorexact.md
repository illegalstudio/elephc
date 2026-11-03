---
title: "imagecolorexact()"
description: "Returns the palette index of an exact color, or -1."
---

## imagecolorexact()

```php
function imagecolorexact(mixed $image, int $red, int $green, int $blue): int
```

Returns the palette index of an exact color, or -1.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorexact` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorexact.md).
