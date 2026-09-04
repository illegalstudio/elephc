---
title: "cairo_set_matrix()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 437
---

## cairo_set_matrix()

```php
function cairo_set_matrix(mixed $context, mixed $matrix): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$matrix` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_matrix` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_matrix.md).
