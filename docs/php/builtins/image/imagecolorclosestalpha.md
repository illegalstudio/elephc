---
title: "imagecolorclosestalpha()"
description: "Returns the palette index closest to the requested color with alpha."
---

## imagecolorclosestalpha()

```php
function imagecolorclosestalpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Returns the palette index closest to the requested color with alpha.

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

For how `imagecolorclosestalpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorclosestalpha.md).
