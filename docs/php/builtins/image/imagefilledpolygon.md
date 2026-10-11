---
title: "imagefilledpolygon()"
description: "Draws a filled polygon."
sidebar:
  order: 507
---

## imagefilledpolygon()

```php
function imagefilledpolygon(mixed $image, array $points, int $color): bool
```

Draws a filled polygon.

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

For how `imagefilledpolygon` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefilledpolygon.md).
