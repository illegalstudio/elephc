---
title: "imagerotate()"
description: "Returns a copy of an image rotated by an angle."
sidebar:
  order: 527
---

## imagerotate()

```php
function imagerotate(mixed $image, float $angle, int $background_color, int $ignore_transparent = 0): mixed
```

Returns a copy of an image rotated by an angle.

**Parameters**:
- `$image` (`mixed`)
- `$angle` (`float`)
- `$background_color` (`int`)
- `$ignore_transparent` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagerotate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagerotate.md).
