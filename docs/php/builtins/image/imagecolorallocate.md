---
title: "imagecolorallocate()"
description: "Allocates an opaque color in a palette image."
sidebar:
  order: 466
---

## imagecolorallocate()

```php
function imagecolorallocate(mixed $image, int $red, int $green, int $blue): int
```

Allocates an opaque color in a palette image.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorallocate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorallocate.md).
