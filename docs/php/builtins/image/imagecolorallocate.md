---
title: "imagecolorallocate()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 463
---

## imagecolorallocate()

```php
function imagecolorallocate(mixed $image, int $red, int $green, int $blue): int
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorallocate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorallocate.md).
