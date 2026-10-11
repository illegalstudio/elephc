---
title: "cairo_transform()"
description: "Composes the given matrix onto the context's transformation."
sidebar:
  order: 449
---

## cairo_transform()

```php
function cairo_transform(mixed $context, mixed $matrix): void
```

Composes the given matrix onto the context's transformation.

**Parameters**:
- `$context` (`mixed`)
- `$matrix` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_transform` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_transform.md).
