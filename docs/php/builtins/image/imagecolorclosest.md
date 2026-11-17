---
title: "imagecolorclosest()"
description: "Returns the palette index closest to the requested color."
---

## imagecolorclosest()

```php
function imagecolorclosest(mixed $image, int $red, int $green, int $blue): int
```

Returns the palette index closest to the requested color.

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

For how `imagecolorclosest` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorclosest.md).
