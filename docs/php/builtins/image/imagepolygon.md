---
title: "imagepolygon()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 521
---

## imagepolygon()

```php
function imagepolygon(mixed $image, mixed $points, int $color): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$points` (`mixed`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagepolygon` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagepolygon.md).
