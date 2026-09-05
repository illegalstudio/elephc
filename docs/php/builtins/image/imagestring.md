---
title: "imagestring()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 530
---

## imagestring()

```php
function imagestring(mixed $image, int $font, int $x, int $y, string $string, int $color): bool
```

Implemented by the compiler-injected image prelude.

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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagestring` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagestring.md).
