---
title: "imagecolorresolve()"
description: "Returns the palette index of a color, allocating or approximating it."
---

## imagecolorresolve()

```php
function imagecolorresolve(mixed $image, int $red, int $green, int $blue): int
```

Returns the palette index of a color, allocating or approximating it.

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

For how `imagecolorresolve` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorresolve.md).
