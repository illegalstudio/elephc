---
title: "imagecolorset()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 475
---

## imagecolorset()

```php
function imagecolorset(mixed $image, int $color, int $red, int $green, int $blue, int $alpha = 0): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`int`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)
- `$alpha` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorset` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorset.md).
