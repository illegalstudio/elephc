---
title: "imagecolordeallocate()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 469
---

## imagecolordeallocate()

```php
function imagecolordeallocate(mixed $image, int $color): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolordeallocate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolordeallocate.md).
