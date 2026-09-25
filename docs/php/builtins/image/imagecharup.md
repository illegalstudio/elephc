---
title: "imagecharup()"
description: "Draws one character vertically with a built-in font."
sidebar:
  order: 467
---

## imagecharup()

```php
function imagecharup(mixed $image, int $font, int $x, int $y, string $char, int $color): bool
```

Draws one character vertically with a built-in font.

**Parameters**:
- `$image` (`mixed`)
- `$font` (`int`)
- `$x` (`int`)
- `$y` (`int`)
- `$char` (`string`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecharup` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecharup.md).
