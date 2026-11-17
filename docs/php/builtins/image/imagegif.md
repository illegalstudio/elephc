---
title: "imagegif()"
description: "Writes an image as GIF, to a file or to the output buffer."
---

## imagegif()

```php
function imagegif(mixed $image, ?string $file = null): bool
```

Writes an image as GIF, to a file or to the output buffer.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegif` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegif.md).
