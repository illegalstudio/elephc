---
title: "cairo_move_to()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 418
---

## cairo_move_to()

```php
function cairo_move_to(mixed $context, float $x, float $y): void
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$context` (`mixed`)
- `$x` (`float`)
- `$y` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_move_to` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_move_to.md).
