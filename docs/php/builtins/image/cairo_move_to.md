---
title: "cairo_move_to()"
description: "Begins a new subpath at the given point."
sidebar:
  order: 421
---

## cairo_move_to()

```php
function cairo_move_to(mixed $context, float $x, float $y): void
```

Begins a new subpath at the given point.

**Parameters**:
- `$context` (`mixed`)
- `$x` (`float`)
- `$y` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_move_to` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_move_to.md).
