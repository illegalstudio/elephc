---
title: "imagecolorclosesthwb()"
description: "Returns the palette index closest in hue, whiteness, and blackness."
---

## imagecolorclosesthwb()

```php
function imagecolorclosesthwb(mixed $image, int $red, int $green, int $blue): int
```

Returns the palette index closest in hue, whiteness, and blackness.

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

For how `imagecolorclosesthwb` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorclosesthwb.md).
