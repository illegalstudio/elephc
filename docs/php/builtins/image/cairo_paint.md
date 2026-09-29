---
title: "cairo_paint()"
description: "Paints the current source over the whole clip region."
sidebar:
  order: 424
---

## cairo_paint()

```php
function cairo_paint(mixed $context): void
```

Paints the current source over the whole clip region.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_paint` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_paint.md).
