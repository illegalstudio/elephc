---
title: "imagesetpixel()"
description: "Sets one pixel to a color."
sidebar:
  order: 531
---

## imagesetpixel()

```php
function imagesetpixel(mixed $image, int $x, int $y, int $color): bool
```

Sets one pixel to a color.

**Parameters**:
- `$image` (`mixed`)
- `$x` (`int`)
- `$y` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesetpixel` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesetpixel.md).
