---
title: "imageaffine()"
description: "Returns a copy of an image with an affine matrix applied."
---

## imageaffine()

```php
function imageaffine(mixed $image, array $affine, ?array $clip = null): mixed
```

Returns a copy of an image with an affine matrix applied.

**Parameters**:
- `$image` (`mixed`)
- `$affine` (`array`)
- `$clip` (`?array`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageaffine` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageaffine.md).
