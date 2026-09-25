---
title: "cairo_scale()"
description: "Scales the context's transformation by the given x and y factors."
sidebar:
  order: 437
---

## cairo_scale()

```php
function cairo_scale(mixed $context, float $sx, float $sy): void
```

Scales the context's transformation by the given x and y factors.

**Parameters**:
- `$context` (`mixed`)
- `$sx` (`float`)
- `$sy` (`float`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cairo_scale` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/cairo_scale.md).
