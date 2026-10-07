---
title: "imagerectangle()"
description: "Draws the outline of a rectangle."
sidebar:
  order: 529
---

## imagerectangle()

```php
function imagerectangle(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
```

Draws the outline of a rectangle.

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

For how `imagerectangle` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagerectangle.md).
