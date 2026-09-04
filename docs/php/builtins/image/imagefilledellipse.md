---
title: "imagefilledellipse()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 501
---

## imagefilledellipse()

```php
function imagefilledellipse(mixed $image, int $center_x, int $center_y, int $width, int $height, int $color): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$center_x` (`int`)
- `$center_y` (`int`)
- `$width` (`int`)
- `$height` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefilledellipse` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefilledellipse.md).
