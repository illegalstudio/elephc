---
title: "cairo_set_line_cap()"
description: "Selects how the ends of a stroked line are drawn."
sidebar:
  order: 439
---

## cairo_set_line_cap()

```php
function cairo_set_line_cap(mixed $context, int $lineCap): void
```

Selects how the ends of a stroked line are drawn.

**Parameters**:
- `$context` (`mixed`)
- `$lineCap` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_line_cap` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_line_cap.md).
