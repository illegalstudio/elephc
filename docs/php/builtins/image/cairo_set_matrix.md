---
title: "cairo_set_matrix()"
description: "Replaces the context's transformation with the given matrix."
sidebar:
  order: 442
---

## cairo_set_matrix()

```php
function cairo_set_matrix(mixed $context, mixed $matrix): void
```

Replaces the context's transformation with the given matrix.

**Parameters**:
- `$context` (`mixed`)
- `$matrix` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_matrix` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_matrix.md).
