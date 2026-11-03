---
title: "imagedestroy()"
description: "Releases an image handle. A no-op since PHP 8.0."
---

## imagedestroy()

```php
function imagedestroy(mixed $image): bool
```

Releases an image handle. A no-op since PHP 8.0.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagedestroy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagedestroy.md).
