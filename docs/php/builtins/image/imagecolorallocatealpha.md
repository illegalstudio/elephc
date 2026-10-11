---
title: "imagecolorallocatealpha()"
description: "Allocates a color with alpha in a palette image."
sidebar:
  order: 469
---

## imagecolorallocatealpha()

```php
function imagecolorallocatealpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Allocates a color with alpha in a palette image.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)
- `$alpha` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorallocatealpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorallocatealpha.md).
