---
title: "imagearc()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 459
---

## imagearc()

```php
function imagearc(mixed $image, int $center_x, int $center_y, int $width, int $height, int $start_angle, int $end_angle, int $color): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$center_x` (`int`)
- `$center_y` (`int`)
- `$width` (`int`)
- `$height` (`int`)
- `$start_angle` (`int`)
- `$end_angle` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagearc` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagearc.md).
