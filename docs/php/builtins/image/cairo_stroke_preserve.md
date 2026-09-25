---
title: "cairo_stroke_preserve()"
description: "Strokes the current path with the current source and keeps the path."
sidebar:
  order: 447
---

## cairo_stroke_preserve()

```php
function cairo_stroke_preserve(mixed $context): void
```

Strokes the current path with the current source and keeps the path.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_stroke_preserve` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_stroke_preserve.md).
