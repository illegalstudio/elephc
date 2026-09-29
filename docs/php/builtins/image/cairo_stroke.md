---
title: "cairo_stroke()"
description: "Strokes the current path with the current source and clears the path."
sidebar:
  order: 444
---

## cairo_stroke()

```php
function cairo_stroke(mixed $context): void
```

Strokes the current path with the current source and clears the path.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_stroke` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_stroke.md).
