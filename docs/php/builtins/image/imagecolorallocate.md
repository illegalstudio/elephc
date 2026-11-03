---
title: "imagecolorallocate()"
description: "Allocates an opaque color in a palette image."
---

## imagecolorallocate()

```php
function imagecolorallocate(mixed $image, int $red, int $green, int $blue): int
```

Allocates an opaque color in a palette image.

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

For how `imagecolorallocate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorallocate.md).
