---
title: "cairo_paint()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 421
---

## cairo_paint()

```php
function cairo_paint(mixed $context): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_paint` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_paint.md).
