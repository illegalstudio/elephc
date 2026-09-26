---
title: "imagedashedline()"
description: "Draws a dashed line. Superseded by imagesetstyle() with imageline()."
sidebar:
  order: 499
---

## imagedashedline()

```php
function imagedashedline(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
```

Draws a dashed line. Superseded by imagesetstyle() with imageline().

**Parameters**:
- `$image` (`mixed`)
- `$x1` (`int`)
- `$y1` (`int`)
- `$x2` (`int`)
- `$y2` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagedashedline` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagedashedline.md).
