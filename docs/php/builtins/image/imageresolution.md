---
title: "imageresolution()"
description: "Reads or sets an image's horizontal and vertical resolution in DPI."
---

## imageresolution()

```php
function imageresolution(mixed $image, ?int $resolution_x = null, ?int $resolution_y = null): mixed
```

Reads or sets an image's horizontal and vertical resolution in DPI.

**Parameters**:
- `$image` (`mixed`)
- `$resolution_x` (`?int`), default `null`, optional
- `$resolution_y` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageresolution` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageresolution.md).
