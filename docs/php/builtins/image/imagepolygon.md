---
title: "imagepolygon()"
description: "Draws the outline of a closed polygon."
sidebar:
  order: 524
---

## imagepolygon()

```php
function imagepolygon(mixed $image, array $points, int $color): bool
```

Draws the outline of a closed polygon.

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

For how `imagepolygon` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagepolygon.md).
