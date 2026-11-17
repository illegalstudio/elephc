---
title: "imagesavealpha()"
description: "Controls whether full alpha channel information is saved with the image."
---

## imagesavealpha()

```php
function imagesavealpha(mixed $image, bool $enable): bool
```

Controls whether full alpha channel information is saved with the image.

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

For how `imagesavealpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesavealpha.md).
