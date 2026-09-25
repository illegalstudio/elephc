---
title: "cairo_matrix_init_scale()"
description: "Creates a matrix that scales by the given x and y factors."
sidebar:
  order: 419
---

## cairo_matrix_init_scale()

```php
function cairo_matrix_init_scale(float $sx, float $sy): mixed
```

Creates a matrix that scales by the given x and y factors.

**Parameters**:
- `$sx` (`float`)
- `$sy` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_matrix_init_scale` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_matrix_init_scale.md).
