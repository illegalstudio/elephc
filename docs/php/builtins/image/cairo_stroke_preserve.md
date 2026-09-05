---
title: "cairo_stroke_preserve()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 442
---

## cairo_stroke_preserve()

```php
function cairo_stroke_preserve(mixed $context): void
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

For how `cairo_stroke_preserve` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_stroke_preserve.md).
