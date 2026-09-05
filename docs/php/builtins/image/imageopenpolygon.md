---
title: "imageopenpolygon()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 517
---

## imageopenpolygon()

```php
function imageopenpolygon(mixed $image, array $points, int $color): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$points` (`array`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageopenpolygon` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageopenpolygon.md).
