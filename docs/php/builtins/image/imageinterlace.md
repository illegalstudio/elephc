---
title: "imageinterlace()"
description: "Reads or sets whether an image is written interlaced."
---

## imageinterlace()

```php
function imageinterlace(mixed $image, ?bool $enable = null): int
```

Reads or sets whether an image is written interlaced.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`?bool`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageinterlace` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageinterlace.md).
