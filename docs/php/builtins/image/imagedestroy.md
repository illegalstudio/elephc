---
title: "imagedestroy()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 497
---

## imagedestroy()

```php
function imagedestroy(mixed $image): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagedestroy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagedestroy.md).
