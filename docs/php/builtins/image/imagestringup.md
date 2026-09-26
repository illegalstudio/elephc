---
title: "imagestringup()"
description: "Draws a string vertically with a built-in font."
sidebar:
  order: 534
---

## imagestringup()

```php
function imagestringup(mixed $image, int $font, int $x, int $y, string $string, int $color): bool
```

Draws a string vertically with a built-in font.

**Parameters**:
- `$image` (`mixed`)
- `$font` (`int`)
- `$x` (`int`)
- `$y` (`int`)
- `$string` (`string`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagestringup` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagestringup.md).
