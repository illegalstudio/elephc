---
title: "cairo_get_current_point()"
description: "Returns the current point of the path as an [x, y] pair."
sidebar:
  order: 410
---

## cairo_get_current_point()

```php
function cairo_get_current_point(mixed $context): array
```

Returns the current point of the path as an [x, y] pair.

**Parameters**:
- `$context` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_get_current_point` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_get_current_point.md).
