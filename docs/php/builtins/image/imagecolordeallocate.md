---
title: "imagecolordeallocate()"
description: "Frees a palette entry allocated earlier."
---

## imagecolordeallocate()

```php
function imagecolordeallocate(mixed $image, int $color): bool
```

Frees a palette entry allocated earlier.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolordeallocate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolordeallocate.md).
