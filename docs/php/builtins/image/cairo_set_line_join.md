---
title: "cairo_set_line_join()"
description: "Selects how corners between stroked segments are drawn."
sidebar:
  order: 438
---

## cairo_set_line_join()

```php
function cairo_set_line_join(mixed $context, int $lineJoin): void
```

Selects how corners between stroked segments are drawn.

**Parameters**:
- `$context` (`mixed`)
- `$lineJoin` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_set_line_join` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_set_line_join.md).
