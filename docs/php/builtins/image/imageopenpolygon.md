---
title: "imageopenpolygon()"
description: "Draws an unclosed polyline."
sidebar:
  order: 524
---

## imageopenpolygon()

```php
function imageopenpolygon(mixed $image, array $points, int $color): bool
```

Draws an unclosed polyline.

**Parameters**:
- `$image` (`mixed`)
- `$points` (`array`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageopenpolygon` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageopenpolygon.md).
