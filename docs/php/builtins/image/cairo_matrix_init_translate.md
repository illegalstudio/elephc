---
title: "cairo_matrix_init_translate()"
description: "Creates a matrix that translates by the given x and y offsets."
sidebar:
  order: 420
---

## cairo_matrix_init_translate()

```php
function cairo_matrix_init_translate(float $tx, float $ty): mixed
```

Creates a matrix that translates by the given x and y offsets.

**Parameters**:
- `$tx` (`float`)
- `$ty` (`float`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_matrix_init_translate` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_matrix_init_translate.md).
