---
title: "imageaffinematrixconcat()"
description: "Returns the product of two affine transformation matrices."
sidebar:
  order: 461
---

## imageaffinematrixconcat()

```php
function imageaffinematrixconcat(array $matrix1, array $matrix2): array
```

Returns the product of two affine transformation matrices.

**Parameters**:
- `$matrix1` (`array`)
- `$matrix2` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageaffinematrixconcat` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageaffinematrixconcat.md).
