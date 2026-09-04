---
title: "imagedashedline()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 496
---

## imagedashedline()

```php
function imagedashedline(mixed $image, int $x1, int $y1, int $x2, int $y2, int $color): bool
```

Implemented by the compiler-injected image prelude.

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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagedashedline` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagedashedline.md).
