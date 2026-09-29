---
title: "imagegetinterpolation()"
description: "Returns the interpolation method used when resampling."
sidebar:
  order: 513
---

## imagegetinterpolation()

```php
function imagegetinterpolation(mixed $image): int
```

Returns the interpolation method used when resampling.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegetinterpolation` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegetinterpolation.md).
