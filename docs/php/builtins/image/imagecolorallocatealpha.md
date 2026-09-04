---
title: "imagecolorallocatealpha()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 464
---

## imagecolorallocatealpha()

```php
function imagecolorallocatealpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Implemented by the compiler-injected image prelude.

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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorallocatealpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorallocatealpha.md).
