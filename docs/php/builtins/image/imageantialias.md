---
title: "imageantialias()"
description: "Turns antialiased drawing on or off for lines and polygons."
---

## imageantialias()

```php
function imageantialias(mixed $image, bool $enable): bool
```

Turns antialiased drawing on or off for lines and polygons.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageantialias` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageantialias.md).
