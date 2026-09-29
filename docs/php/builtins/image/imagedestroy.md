---
title: "imagedestroy()"
description: "Releases an image handle. A no-op since PHP 8.0."
sidebar:
  order: 500
---

## imagedestroy()

```php
function imagedestroy(mixed $image): bool
```

Releases an image handle. A no-op since PHP 8.0.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagedestroy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagedestroy.md).
