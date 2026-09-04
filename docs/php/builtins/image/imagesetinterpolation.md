---
title: "imagesetinterpolation()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 527
---

## imagesetinterpolation()

```php
function imagesetinterpolation(mixed $image, int $method = IMG_BILINEAR_FIXED): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$method` (`int`), default `IMG_BILINEAR_FIXED`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesetinterpolation` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesetinterpolation.md).
