---
title: "cairo_matrix_init_rotate()"
description: "Creates a matrix that rotates by the given angle in radians."
sidebar:
  order: 416
---

## cairo_matrix_init_rotate()

```php
function cairo_matrix_init_rotate(float $radians): mixed
```

Creates a matrix that rotates by the given angle in radians.

**Parameters**:
- `$radians` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_matrix_init_rotate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_matrix_init_rotate.md).
