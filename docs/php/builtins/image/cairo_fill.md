---
title: "cairo_fill()"
description: "Fills the current path with the current source and clears the path."
sidebar:
  order: 408
---

## cairo_fill()

```php
function cairo_fill(mixed $context): void
```

Fills the current path with the current source and clears the path.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_fill` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_fill.md).
