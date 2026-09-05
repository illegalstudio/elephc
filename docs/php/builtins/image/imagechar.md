---
title: "imagechar()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 461
---

## imagechar()

```php
function imagechar(mixed $image, int $font, int $x, int $y, string $char, int $color): bool
```

Implemented by the compiler-injected image prelude.

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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagechar` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagechar.md).
